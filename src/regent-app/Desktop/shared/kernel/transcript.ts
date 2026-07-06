// Pure transcript state for one chat session — the reducer the Chat surface
// renders from. Domain imports nothing from infrastructure: the deacon's wire
// events (message.delta/complete, turn.*, tool.start/complete,
// approval.request) are mapped into ChatEvent by the viewmodel at the boundary.

export type TranscriptItem =
  | { readonly kind: "user"; readonly text: string }
  | { readonly kind: "assistant"; readonly text: string; readonly streaming: boolean }
  | { readonly kind: "thinking"; readonly text: string }
  | { readonly kind: "tool"; readonly name: string; readonly done: boolean; readonly isError?: boolean }
  | {
      readonly kind: "approval";
      readonly tool: string;
      readonly action: string;
      readonly reason: string;
      readonly resolved?: "approved" | "denied";
    }
  | { readonly kind: "error"; readonly message: string };

export interface TranscriptState {
  readonly items: readonly TranscriptItem[];
  /** A turn is in flight (composer shows stop, not send). */
  readonly busy: boolean;
}

export type ChatEvent =
  | { readonly type: "reset" }
  | { readonly type: "seeded"; readonly items: readonly TranscriptItem[] }
  | { readonly type: "submitted"; readonly text: string }
  | { readonly type: "delta"; readonly text: string }
  | { readonly type: "reply"; readonly text: string }
  | { readonly type: "tool-start"; readonly name: string }
  | { readonly type: "tool-end"; readonly name: string; readonly isError?: boolean }
  | { readonly type: "approval"; readonly tool: string; readonly action: string; readonly reason: string }
  | { readonly type: "approval-resolved"; readonly approved: boolean }
  | { readonly type: "ended"; readonly error?: string }
  | { readonly type: "failed"; readonly message: string };

export const emptyTranscript: TranscriptState = { items: [], busy: false };

const sealStreaming = (items: readonly TranscriptItem[]): TranscriptItem[] =>
  items.map((i) => (i.kind === "assistant" && i.streaming ? { ...i, streaming: false } : i));

/** Index just past the last user item — the current turn's start. */
const turnStart = (items: readonly TranscriptItem[]): number => {
  for (let i = items.length - 1; i >= 0; i--) {
    if (items[i].kind === "user") return i + 1;
  }
  return 0;
};

/** Apply one event. Deltas accumulate into a trailing streaming item; a tool
 * row in between starts a fresh one (per-step separation). `reply` carries
 * the WHOLE turn's final text, so it replaces every assistant fragment of the
 * current turn (thinking/tool rows stay). Errors surface verbatim. */
export function reduceTranscript(state: TranscriptState, event: ChatEvent): TranscriptState {
  switch (event.type) {
    case "reset":
      return emptyTranscript;
    case "seeded":
      // Stored history replaces an EMPTY transcript only — a seed arriving
      // after the user already typed must never clobber live turns.
      if (state.items.length > 0) return state;
      return { ...state, items: [...event.items] };
    case "submitted":
      return {
        items: [...sealStreaming(state.items), { kind: "user", text: event.text }],
        busy: true,
      };
    case "delta": {
      const last = state.items.at(-1);
      if (last?.kind === "assistant" && last.streaming) {
        return {
          ...state,
          items: [
            ...state.items.slice(0, -1),
            { kind: "assistant", text: last.text + event.text, streaming: true },
          ],
        };
      }
      return {
        ...state,
        items: [...state.items, { kind: "assistant", text: event.text, streaming: true }],
      };
    }
    case "reply": {
      const start = turnStart(state.items);
      const kept = [
        ...state.items.slice(0, start),
        ...state.items.slice(start).filter((i) => i.kind !== "assistant"),
      ];
      return { ...state, items: [...kept, { kind: "assistant", text: event.text, streaming: false }] };
    }
    case "tool-start":
      return { ...state, items: [...state.items, { kind: "tool", name: event.name, done: false }] };
    case "tool-end": {
      const items = [...state.items];
      for (let i = items.length - 1; i >= 0; i--) {
        const it = items[i];
        if (it.kind === "tool" && it.name === event.name && !it.done) {
          items[i] = { ...it, done: true, isError: event.isError };
          break;
        }
      }
      return { ...state, items };
    }
    case "approval":
      return {
        ...state,
        items: [
          ...state.items,
          { kind: "approval", tool: event.tool, action: event.action, reason: event.reason },
        ],
      };
    case "approval-resolved": {
      const items = [...state.items];
      for (let i = items.length - 1; i >= 0; i--) {
        const it = items[i];
        if (it.kind === "approval" && it.resolved === undefined) {
          items[i] = { ...it, resolved: event.approved ? "approved" : "denied" };
          break;
        }
      }
      return { ...state, items };
    }
    case "ended": {
      const sealed = sealStreaming(state.items).map(
        (i): TranscriptItem => (i.kind === "tool" && !i.done ? { ...i, done: true } : i),
      );
      if (event.error) sealed.push({ kind: "error", message: event.error });
      return { items: sealed, busy: false };
    }
    case "failed":
      return { items: [...state.items, { kind: "error", message: event.message }], busy: false };
  }
}
