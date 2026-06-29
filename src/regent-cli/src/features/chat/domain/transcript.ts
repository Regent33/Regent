// The chat transcript reducer — a pure (state, action) → state fold of daemon
// notifications and local user actions. This is the testable heart of the chat
// surface; it mirrors the Go view.go handleNotif semantics exactly so the two
// front-ends behave identically. No I/O, no framework imports.

export type ChatPhase = "idle" | "busy" | "approving";

export type TranscriptEntry =
  | { id: number; kind: "user"; text: string }
  // `final` = the authoritative end-of-turn reply (framed); mid-turn preambles
  // committed when a tool interrupts are non-final (rendered plain, not boxed).
  | { id: number; kind: "assistant"; text: string; final: boolean }
  | { id: number; kind: "tool"; tool: string }
  | { id: number; kind: "toolError"; tool: string }
  | { id: number; kind: "approvalAsk"; tool: string; action: string }
  | { id: number; kind: "approvalResolved"; approved: boolean }
  | { id: number; kind: "outbound"; target: string; text: string }
  | { id: number; kind: "note"; text: string };

export interface ChatState {
  readonly entries: readonly TranscriptEntry[];
  readonly streaming: string;
  readonly streamingActive: boolean;
  readonly phase: ChatPhase;
  readonly approval: { readonly tool: string; readonly action: string } | null;
  readonly nextId: number;
  // Latest context usage (for the status-line fill bar), from `turn.usage`.
  readonly contextTokens: number;
  readonly maxContextTokens: number;
  readonly model: string;
}

export const initialChatState: ChatState = {
  entries: [],
  streaming: "",
  streamingActive: false,
  phase: "idle",
  approval: null,
  nextId: 0,
  contextTokens: 0,
  maxContextTokens: 0,
  model: "",
};

export type ChatAction =
  | { type: "userMessage"; text: string }
  | { type: "approvalResolved"; approved: boolean }
  | { type: "note"; text: string }
  | { type: "reset" }
  | { type: "streamClosed" }
  | { type: "daemonEvent"; method: string; params: Record<string, unknown> };

// Distributive Omit: a plain Omit<Union, K> collapses to the union's common
// keys, dropping kind-specific fields. This preserves each variant.
type NewEntry = TranscriptEntry extends infer T
  ? T extends TranscriptEntry
    ? Omit<T, "id">
    : never
  : never;

// Append an entry, assigning it the next id.
function withEntry(s: ChatState, e: NewEntry): ChatState {
  const entry = { id: s.nextId, ...e } as TranscriptEntry;
  return { ...s, entries: [...s.entries, entry], nextId: s.nextId + 1 };
}

// Move the live streaming buffer into the transcript as an assistant entry.
function commit(s: ChatState): ChatState {
  if (s.streamingActive && s.streaming.length > 0) {
    const committed = withEntry(s, { kind: "assistant", text: s.streaming, final: false });
    return { ...committed, streaming: "", streamingActive: false };
  }
  return { ...s, streaming: "", streamingActive: false };
}

const str = (params: Record<string, unknown>, key: string): string =>
  typeof params[key] === "string" ? (params[key] as string) : "";

const num = (params: Record<string, unknown>, key: string): number =>
  typeof params[key] === "number" ? (params[key] as number) : 0;

// Length of the shared leading run of two strings.
function commonPrefixLength(a: string, b: string): number {
  const n = Math.min(a.length, b.length);
  let i = 0;
  while (i < n && a[i] === b[i]) i++;
  return i;
}

// Whether the authoritative `final` reply supersedes an earlier in-turn streamed
// `partial`, so it's not shown twice. True for an exact prefix (streamed-then-
// extended) OR a long shared prefix — the model revised the same answer across
// tool rounds (e.g. added a reference), so it's not a byte-for-byte prefix.
function supersedes(final: string, partial: string): boolean {
  if (partial.length === 0) return false;
  if (final.startsWith(partial)) return true;
  return commonPrefixLength(final, partial) >= Math.max(24, Math.floor(partial.length * 0.5));
}

export function reduceChat(state: ChatState, action: ChatAction): ChatState {
  switch (action.type) {
    case "userMessage":
      return { ...withEntry(state, { kind: "user", text: action.text }), phase: "busy" };
    case "approvalResolved":
      return {
        ...withEntry(state, { kind: "approvalResolved", approved: action.approved }),
        approval: null,
        phase: "busy",
      };
    case "note":
      return withEntry(state, { kind: "note", text: action.text });
    case "reset":
      return initialChatState;
    case "streamClosed":
      return withEntry(state, { kind: "note", text: "daemon stream closed" });
    case "daemonEvent":
      return reduceEvent(state, action.method, action.params);
  }
}

function reduceEvent(s: ChatState, method: string, params: Record<string, unknown>): ChatState {
  switch (method) {
    case "turn.started":
      return { ...s, phase: "busy" };
    case "message.delta":
      return { ...s, streaming: s.streaming + str(params, "text"), streamingActive: true };
    case "tool.start":
      return withEntry(commit(s), { kind: "tool", tool: str(params, "tool") });
    case "tool.complete":
      return params.is_error === true
        ? withEntry(s, { kind: "toolError", tool: str(params, "tool") })
        : s;
    case "approval.request": {
      const tool = str(params, "tool");
      const actionText = str(params, "action");
      const c = withEntry(commit(s), { kind: "approvalAsk", tool, action: actionText });
      return { ...c, phase: "approving", approval: { tool, action: actionText } };
    }
    case "message.outbound":
      return withEntry(commit(s), {
        kind: "outbound",
        target: str(params, "target"),
        text: str(params, "text"),
      });
    case "turn.interrupted":
      return { ...withEntry(commit(s), { kind: "note", text: "🛑 interrupted" }), phase: "idle" };
    case "message.complete": {
      // The daemon always sends the authoritative `reply` here (and also streams
      // it via deltas). Commit the reply once, discarding the live preview —
      // falling back to the streamed buffer only if no reply was carried. If the
      // model already emitted this answer mid-turn (streamed then committed
      // before a tool call), the most recent assistant entry is a prefix of the
      // reply: drop it so the final answer isn't shown twice.
      const text = str(params, "reply") || s.streaming;
      if (!text) return { ...s, streaming: "", streamingActive: false };
      let entries = s.entries;
      for (let i = entries.length - 1; i >= 0; i--) {
        const e = entries[i];
        if (e?.kind === "user") break; // stay in this turn — never touch a prior one
        if (e?.kind !== "assistant") continue; // skip tool/note entries between
        if (supersedes(text, e.text)) {
          entries = [...entries.slice(0, i), ...entries.slice(i + 1)];
        }
        break; // only the most recent assistant entry can be a partial of this
      }
      return {
        ...withEntry({ ...s, entries }, { kind: "assistant", text, final: true }),
        streaming: "",
        streamingActive: false,
      };
    }
    case "turn.usage":
      return {
        ...s,
        contextTokens: num(params, "context_tokens"),
        maxContextTokens: num(params, "max_context_tokens"),
        model: str(params, "model") || s.model,
      };
    case "turn.complete":
      return { ...commit(s), phase: "idle" };
    default:
      return s;
  }
}
