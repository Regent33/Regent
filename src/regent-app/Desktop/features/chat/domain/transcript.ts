// Pure transcript state for one chat session — the reducer the Chat surface
// renders from. Domain imports nothing from infrastructure: the deacon's wire
// events (message.delta / message.complete / turn.complete / turn.interrupted)
// are mapped into ChatEvent by the viewmodel at the boundary.

export type TranscriptItem =
  | { readonly kind: "user"; readonly text: string }
  | { readonly kind: "assistant"; readonly text: string; readonly streaming: boolean }
  | { readonly kind: "error"; readonly message: string };

export interface TranscriptState {
  readonly items: readonly TranscriptItem[];
  /** A turn is in flight (composer shows stop, not send). */
  readonly busy: boolean;
}

export type ChatEvent =
  | { readonly type: "submitted"; readonly text: string }
  | { readonly type: "delta"; readonly text: string }
  | { readonly type: "reply"; readonly text: string }
  | { readonly type: "ended"; readonly error?: string }
  | { readonly type: "failed"; readonly message: string };

export const emptyTranscript: TranscriptState = { items: [], busy: false };

/** Apply one event. Deltas accumulate into a trailing streaming item;
 * `reply` replaces it wholesale (non-streaming providers); `ended` seals it.
 * Errors surface verbatim as transcript items — never swallowed. */
export function reduceTranscript(state: TranscriptState, event: ChatEvent): TranscriptState {
  switch (event.type) {
    case "submitted":
      return {
        items: [...state.items, { kind: "user", text: event.text }],
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
      const last = state.items.at(-1);
      const kept = last?.kind === "assistant" && last.streaming ? state.items.slice(0, -1) : state.items;
      return { ...state, items: [...kept, { kind: "assistant", text: event.text, streaming: false }] };
    }
    case "ended": {
      const last = state.items.at(-1);
      const sealed =
        last?.kind === "assistant" && last.streaming
          ? [...state.items.slice(0, -1), { kind: "assistant" as const, text: last.text, streaming: false }]
          : [...state.items];
      if (event.error) sealed.push({ kind: "error", message: event.error });
      return { items: sealed, busy: false };
    }
    case "failed":
      return { items: [...state.items, { kind: "error", message: event.message }], busy: false };
  }
}
