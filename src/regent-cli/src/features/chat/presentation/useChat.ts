import { COPY } from "@app/config/brand.ts";
import type { ChatPort } from "@features/chat/domain/chatPort.ts";
import { type ChatState, initialChatState, reduceChat } from "@features/chat/domain/transcript.ts";
// Chat viewmodel: subscribes deacon events into the transcript reducer and
// exposes the three user actions. All transcript mutation goes through the pure
// reducer; this hook only wires the port and dispatches.
import { useEffect, useReducer, useRef } from "react";

// Redrawing Ink's live region per streaming token thrashes the terminal (CPU +
// jank, and you can't stay scrolled up while it redraws). Coalesce delta text
// and flush at ~20fps; concatenated deltas reduce to the same state, so output
// is identical — just fewer frames.
const DELTA_FLUSH_MS = 50;

export interface ChatViewModel {
  readonly state: ChatState;
  readonly sendPrompt: (text: string) => void;
  readonly interrupt: () => void;
  readonly respond: (approved: boolean) => void;
  /** Append a local note to the transcript (slash-command output). */
  readonly note: (text: string) => void;
  /** Clear the transcript (the `/new` command). */
  readonly reset: () => void;
}

export function useChat(port: ChatPort, sessionId: string): ChatViewModel {
  const [state, dispatch] = useReducer(reduceChat, initialChatState);
  const deltaBuf = useRef("");
  const flushTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    const flushDeltas = () => {
      if (flushTimer.current) {
        clearTimeout(flushTimer.current);
        flushTimer.current = null;
      }
      if (deltaBuf.current) {
        const text = deltaBuf.current;
        deltaBuf.current = "";
        dispatch({ type: "deaconEvent", method: "message.delta", params: { text } });
      }
    };

    const unsub = port.onEvent((event) => {
      const sid = event.params.session_id;
      if (typeof sid === "string" && sid !== sessionId) return; // ignore other sessions
      if (event.method === "message.delta") {
        deltaBuf.current += typeof event.params.text === "string" ? event.params.text : "";
        if (!flushTimer.current) flushTimer.current = setTimeout(flushDeltas, DELTA_FLUSH_MS);
        return;
      }
      // Any non-delta event (tool.start, message.complete, …) commits buffered
      // text first so transcript ordering is preserved.
      flushDeltas();
      dispatch({ type: "deaconEvent", method: event.method, params: event.params });
    });

    return () => {
      if (flushTimer.current) clearTimeout(flushTimer.current);
      flushTimer.current = null;
      deltaBuf.current = "";
      unsub();
    };
  }, [port, sessionId]);

  const sendPrompt = (text: string) => {
    dispatch({ type: "userMessage", text });
    void port.submit(text).then((res) => {
      // Backstop: the reply streams via events; surface only an error they
      // didn't carry (mirrors the Go respMsg handling).
      if (!res.ok) dispatch({ type: "note", text: COPY.submitError(res.error.message) });
    });
  };

  const interrupt = () => {
    void port.interrupt();
  };

  const respond = (approved: boolean) => {
    dispatch({ type: "approvalResolved", approved });
    void port.respondApproval(approved);
  };

  const note = (text: string) => dispatch({ type: "note", text });
  const reset = () => dispatch({ type: "reset" });

  return { state, sendPrompt, interrupt, respond, note, reset };
}
