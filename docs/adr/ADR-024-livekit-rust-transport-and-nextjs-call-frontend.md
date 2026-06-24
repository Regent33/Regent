# ADR-024: Real-time calls — LiveKit-Rust transport + a Next.js "Jarvis" frontend

**Status:** Accepted — 2026-06-24

**Context:** ADR-021 chose LiveKit/WebRTC as the first real-time transport and a
speech-to-speech provider as the brain, with "a tiny web page" as the client
(proposal R2). We needed to pick the concrete client stack and confirm the Rust
LiveKit SDK is viable on the user's Windows box (the whole reason to avoid
Hermes's virtual-audio hack). The old `python-voice-server` is **turn-based**, not
a live duplex call.

**Decision:** Implement the LiveKit transport in **Rust** against the official
`livekit` SDK (0.7), kept **optional + behind a `livekit` Cargo feature** because it
pulls native libwebrtc — the default `cargo build --workspace` never touches it.
Build the call client as a **standalone Next.js 16 / React 19 app** (`src/regent-web`),
required stack: **Tailwind v4, three.js (R3F), React Spring, GSAP**. The voice
animation is a **braille-style dot field** (canvas, audio-reactive). A server-side
token route (`livekit-server-sdk`) signs join JWTs from env, so it works with
self-hosted LiveKit OSS or LiveKit Cloud. `regent call serve` launches it all.

**Consequences:** The native LiveKit SDK **compiled on Windows** (verified), so the
Rust transport is real, not a stub. The UI degrades to a **local-mic preview** with
no server/key, so it's demonstrable immediately. Cost: a new Node/Next toolchain
beside the Bun CLI + Rust — isolated under `src/regent-web`, no impact on existing
builds. `python-voice-server` stays for turn-based local speech.
