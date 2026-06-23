# ADR-021: Real-time calls — a Realtime-API engine, LiveKit first, Telegram second

**Status:** Proposed — 2026-06-23

**Context:** The user wants real-time voice *calls* (not just turn-based voice
messages), long-term, over both LiveKit/WebRTC and Telegram. ADR-018 deferred
real-time to a later phase; this is that phase. Studying Hermes's real-time mode
(`plugins/google_meet`) showed it does **not** hand-roll STT→LLM→TTS — it uses the
**OpenAI Realtime API** (`gpt-realtime`, speech-to-speech over one WebSocket, with
built-in VAD/turn/barge-in) and pipes audio through browser automation + virtual
audio devices (PulseAudio/BlackHole) — which Hermes itself marks **untested on
Windows**.

**Decision:** Build a `regent-realtime` crate around two traits:
**`RealtimeProvider`** (the speech-to-speech brain — OpenAI Realtime first, Gemini
Live swappable; the API handles VAD/turn/barge-in) and **`RealtimeTransport`**
(audio frames + control events). The engine relays transport audio ⇄ the provider
and bridges the provider's **function-calling** to Regent's existing tools + graph
memory, with per-call/day **spend caps**. **LiveKit/WebRTC is the first transport**
(clean audio tracks, real echo cancellation, **Windows-friendly** — no virtual
audio devices); **Telegram MTProto is second**, via a `pytgcalls` Python sidecar
(user account, group voice-chats, opt-in). A **local/offline** `RealtimeProvider`
(hand-rolled VAD + streaming STT/TTS) is the deferred fallback for privacy/Qwen3.

**Consequences:** We don't reinvent VAD/turn-taking/barge-in — the Realtime API
owns the hard real-time speech parts; the engineering is the transport contract +
tool bridge (both pure/testable against mocks). Cost is per audio-minute (capped);
a Realtime API key is required for the easy path. LiveKit avoids Hermes's
virtual-audio hack, so it runs on the user's Windows box. R0 (engine + mock) is
fully offline-testable before any heavy SDK lands.
