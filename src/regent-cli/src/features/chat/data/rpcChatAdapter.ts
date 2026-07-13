// ChatPort over the JSON-RPC client, scoped to one session. Method/param shapes
// match the deacon contract the Go CLI uses (chat.go): prompt.submit,
// turn.interrupt, approval.respond.
import type { ChatPort } from "@features/chat/domain/chatPort.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";

export function createRpcChatAdapter(client: IRpcClient, sessionId: string): ChatPort {
  return {
    // timeoutMs 0 → no client-side timeout; the turn may run for minutes.
    submit: (text) => client.call("prompt.submit", { session_id: sessionId, text }, 0),
    interrupt: () => client.call("turn.interrupt", { session_id: sessionId }, 10_000),
    respondApproval: (approved, feedback) =>
      client.call(
        "approval.respond",
        { session_id: sessionId, approved, ...(feedback ? { feedback } : {}) },
        10_000,
      ),
    onEvent: (handler) => client.onNotification(handler),
  };
}
