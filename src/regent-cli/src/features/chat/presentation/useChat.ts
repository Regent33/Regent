import { COPY } from "@app/config/brand.ts";
import type { ChatPort } from "@features/chat/domain/chatPort.ts";
import { type ChatState, initialChatState, reduceChat } from "@features/chat/domain/transcript.ts";
// Chat viewmodel: subscribes daemon events into the transcript reducer and
// exposes the three user actions. All transcript mutation goes through the pure
// reducer; this hook only wires the port and dispatches.
import { useEffect, useReducer } from "react";

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

  useEffect(() => {
    return port.onEvent((event) => {
      const sid = event.params.session_id;
      if (typeof sid === "string" && sid !== sessionId) return; // ignore other sessions
      dispatch({ type: "daemonEvent", method: event.method, params: event.params });
    });
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
