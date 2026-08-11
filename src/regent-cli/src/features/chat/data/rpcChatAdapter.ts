// ChatPort over the JSON-RPC client, scoped to one session. Method/param shapes
// match the deacon contract the Go CLI uses (chat.go): prompt.submit,
// turn.interrupt, approval.respond.
import type { ChatPort, UnreportedJob } from "@features/chat/domain/chatPort.ts";
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
    // Deliberately unscoped by session: a background job outlives the turn that
    // started it, and the deacon delivers its outcome to whichever chat speaks
    // next. Scoping the replay would drop exactly the news a restart lost.
    unreportedJobs: () => client.call<UnreportedJob[]>("job.list", {}, 10_000),
  };
}
