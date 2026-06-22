# ADR-017: Pluggable speech stack — kernel traits, registry, disabled by default

**Status:** Proposed — 2026-06-22

**Context:** Voice needs ASR (speech→text) and TTS (text→speech). Requirements:
default to **Qwen3-ASR + Qwen3-TTS** but work with *any* model; ship **off** with **no**
model downloaded; turn on via one intuitive command. Hermes solves the flexibility with
`transcription_registry`/`tts_registry` (built-ins-always-win + plugin override);
`regent-embed` already solves local-model-with-download behind a kernel trait.

**Decision:** Put `AsrProvider`/`TtsProvider` traits in `regent-kernel` beside
`EmbeddingProvider`, carrying Hermes's full ABC surface — not just `transcribe`/
`synthesize` but `is_available()`, `list_models()`/`list_voices()`, `setup_schema()`
(drives the wizard's key prompts), and TTS `stream()` + `voice_compatible` — because the
registry, the `voice` wizard, and the gateway voice-bubble path all depend on those
extras. Implementations live in a new `regent-speech` crate (local Qwen3 via candle/ONNX
+ remote Whisper/ElevenLabs HTTP backends) behind a built-ins-always-win registry ported
from Hermes. **Three flexibility tiers** answer "any model": built-ins, **`command`-type
providers** (a config-declared shell template wrapping any local CLI — Hermes PR-#17843,
wins over a same-name plugin), and registered plugins. The crate also ports Hermes's
hard-won robustness (Whisper hallucination filter, oversized-file chunking, energy VAD).
A model manager downloads/verifies/caches into `$REGENT_HOME/models/` — the explicit,
gated form of `regent-embed`'s auto-download. The feature is **`speech.enabled: false` by
default** (same opt-in shape as `http.enabled`/`board.enabled`); a fresh daemon
loads/downloads nothing. One command, `regent voice setup`, picks providers (defaults
**Qwen3-ASR/Qwen3-TTS**, the speech-capable models — not the dense text model), prompts
for keys via `setup_schema()`, downloads with progress, verifies, and flips the toggle.

**Default backend (settled by studying Hermes):** Hermes serves Groq and OpenAI STT over
the *same* OpenAI-compatible wire (`{base}/audio/transcriptions`, multipart `file`+`model`),
differing only by `base_url`+key, and OpenAI TTS over `{base}/audio/speech`. Qwen3-ASR/TTS
are offered the same way via **Alibaba DashScope's OpenAI-compatible mode**, so the default
is a **single OpenAI-compatible adapter** (`OpenAiCompatAsr`/`OpenAiCompatTts`, mirroring
`regent-providers`' one-adapter-many-base-URLs chat design) pointed at DashScope — and the
identical adapter covers OpenAI/Groq Whisper. Local/offline ASR/TTS is the `command`
provider (whisper.cpp / piper) or a later `whisper-rs` native backend — Regent does **not**
hand-roll model inference (neither does Hermes). The HTTP call is **injected** so
`regent-speech` stays network-free and unit-testable; the daemon supplies the reqwest
executor.

**Consequences:** The model is swappable from config and the composition root; the
agent never sees the backend. The `command`-type tier means even an unsupported model is
one config line, never a code change. No multi-GB surprise on install. Remote backends
are the always-available fallback if a local checkpoint is unproven on an OS.
