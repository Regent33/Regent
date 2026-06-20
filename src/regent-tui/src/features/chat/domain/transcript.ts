// The chat transcript reducer — a pure (state, action) → state fold of daemon
// notifications and local user actions. This is the testable heart of the chat
// surface; it mirrors the Go view.go handleNotif semantics exactly so the two
// front-ends behave identically. No I/O, no framework imports.

export type ChatPhase = "idle" | "busy" | "approving";

export type TranscriptEntry =
  | { id: number; kind: "user"; text: string }
  | { id: number; kind: "assistant"; text: string }
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
}

export const initialChatState: ChatState = {
  entries: [],
  streaming: "",
  streamingActive: false,
  phase: "idle",
  approval: null,
  nextId: 0,
};

export type ChatAction =
  | { type: "userMessage"; text: string }
  | { type: "approvalResolved"; approved: boolean }
  | { type: "note"; text: string }
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
    const committed = withEntry(s, { kind: "assistant", text: s.streaming });
    return { ...committed, streaming: "", streamingActive: false };
  }
  return { ...s, streaming: "", streamingActive: false };
}

const str = (params: Record<string, unknown>, key: string): string =>
  typeof params[key] === "string" ? (params[key] as string) : "";

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
      if (!s.streamingActive) {
        const reply = str(params, "reply");
        if (reply) return commit(withEntry(s, { kind: "assistant", text: reply }));
      }
      return commit(s);
    }
    case "turn.complete":
      return { ...commit(s), phase: "idle" };
    default:
      return s;
  }
}
