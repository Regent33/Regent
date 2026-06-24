# ADR-024: File-send on webhook platforms — separate trait + per-conversation delivery sink

**Status:** Accepted — 2026-06-24

**Context:** The webhook platforms (Slack, WhatsApp, Google Chat, WeChat, Line)
implement the **pure, sync** `WebhookAdapter` (verify → parse → build a
`SendRequest` the shared executor runs) — text-only. Two gaps blocked the agent
from sending files there:

1. **No outbound path.** A webhook conversation runs as a daemon keyed session
   whose `DeliverySink` is `NotificationDelivery` — it emits JSON-RPC
   `message.outbound` to a connected CLI, which no webhook turn has. So the
   agent's `send_message`/`send_file` tools vanished; only the synchronous reply
   (returned from `chat_keyed`) reached the platform.
2. **No upload shape.** Every platform's upload is async + multi-step (WhatsApp
   2-step, Slack 3-step, WeChat upload→`media_id`) — a single pure `SendRequest`
   can't express it, and adding an async method to `WebhookAdapter` would force
   `#[async_trait]` onto all ~16 adapters.

**Decision:**
- **Capability via a separate trait.** New `WebhookFileSender` (async) in the
  gateway, implemented only by platforms with an upload API. The pure `WebhookAdapter`
  stays sync and untouched; the executor injects its `reqwest::Client` so adapters
  stay stateless and the request/response shapes stay unit-testable.
- **Delivery via a per-conversation sink.** New daemon `PlatformDelivery` resolver
  maps a keyed session (`"platform:chat_id"`) to a `WebhookDelivery` sink that
  routes the agent's `send_message` **and** `send_file` back to that platform's
  API. Keyed platform sessions register both tools with it; local CLI sessions are
  unchanged (`NotificationDelivery`, no file tool). The conversation key is threaded
  through additive `create/resume_session_keyed` variants — no `SessionManager::new`
  signature change.

**Consequences:** Adding a platform = implement `WebhookFileSender` + register it in
`file_senders_from_env`; nothing else changes. WhatsApp/Slack/WeChat ship on this
seam. **Google Chat** (sync-reply bot, no outbound token — needs a service-account +
the Chat REST API) and **Line** (URL-only media, no upload API — needs a file host)
are out of reach until that infra exists; they simply have no file sender and
`send_file` declines for them. Outbound text now also reaches webhook platforms (a
side benefit, previously dropped). Complements ADR-016 (inbound media envelope) and
ADR-009 (gateway architecture).
