# ADR-018: Real-time calls — turn-based first, WebRTC + Telegram MTProto later

**Status:** Proposed — 2026-06-22

**Context:** "Voice/video calls to Telegram" has two readings with very different cost.
Turn-based (voice **message** in → voice message out) works on the **Telegram Bot API
today** (`getFile`/`sendVoice`) and matches the existing `twilio_voice` turn shape.
True real-time duplex calls need a media stack (WebRTC, barge-in, streaming ASR/TTS)
and, for Telegram specifically, **MTProto** with a user account (`grammers`) plus its
proprietary voip protocol — the Bot API exposes no real-time call path.

**Decision:** Build the speech/vision engine **transport-agnostic** and ship
**Tier 1 (turn-based)** over the Bot API first (proposal V1–V3). Isolate **Tier 2
(real-time)** in a later, separately-gated `regent-realtime` crate (V4) with a
transport trait: a **generic WebRTC** impl first (broadly reusable, testable behind a
mock), then a **Telegram MTProto** impl as a specialization. Real-time vision is frame
sampling at a capped rate through the same vision routing (ADR-019). Tier 2 ships
`realtime.enabled: false` and is never required for Tier 1.

**Consequences:** The headline ask (talk to the agent by voice on Telegram, agent sees
what you send) lands incrementally without the media-server lift. Real-time UX is
additive and contained; if MTProto proves too heavy, the generic WebRTC/SIP transport
still delivers real-time calls. No early commitment to a media server we may not need.
