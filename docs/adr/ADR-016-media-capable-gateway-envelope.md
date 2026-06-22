# ADR-016: Media-capable gateway envelope — additive attachments, text path unchanged

**Status:** Proposed — 2026-06-22

**Context:** `MessageEvent` and `OutboundMessage` are text-only
(`{ platform, chat_id, user_id, text }` / `{ chat_id, text }`). Voice/video/vision
need media to flow through the same gateway runner, but `MessageEvent`/`OutboundMessage`
are a shared contract used by every platform adapter, the runner, and the tests — the
operating-loop gate forbids breaking it.

**Decision:** Extend both **additively**. Add `MediaRef { kind, ref, mime, duration_ms }`
and `MessageEvent.attachments: Vec<MediaRef>` (default empty), plus
`OutboundMessage.media: Option<OutboundMedia>` and a `reply_modality` (text|voice,
default text). Existing construction sites get `..Default::default()`; the text-only
wire path (Telegram `send_payload`, Twilio TwiML, etc.) is **byte-identical**. Inbound
audio is transcribed and outbound voice is synthesized **in the runner** (the
`twilio_voice` pattern), so `ConversationHandler` stays text-native — media never
forces a handler-contract change. Vision is the one exception (pixels can't flatten to
the runner) and is handled by ADR-019, not here.

**Consequences:** Adapters opt in by populating `attachments` and implementing
`send_voice`/`get_file`; adapters that don't are unchanged. No existing test changes
behavior. The envelope is the single extension point for every future modality, so we
never re-open this contract per platform.
