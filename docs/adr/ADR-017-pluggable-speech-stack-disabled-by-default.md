# ADR-017: Pluggable speech stack — kernel traits, registry, disabled by default

**Status:** Proposed — 2026-06-22

**Context:** Voice needs ASR (speech→text) and TTS (text→speech). Requirements:
default to **Qwen3-ASR-1.7B + Qwen3-TTS-1.7B** but work with *any* model; ship **off**
with **no** model downloaded; turn on via one intuitive command that downloads the
weights. Hermes solves the flexibility with
`transcription_registry`/`tts_registry` (built-ins-always-win + plugin override);
`regent-embed` already solves local-model-with-download behind a kernel trait.

**Decision:** Put `AsrProvider`/`TtsProvider` traits in `regent-kernel` beside
`EmbeddingProvider`, carrying Hermes's full ABC surface — not just `transcribe`/
`synthesize` but `is_available()`, `list_models()`/`list_voices()`, `setup_schema()`
(drives the wizard's key prompts), and TTS `stream()` + `voice_compatible` — because the
registry, the `voice` wizard, and the gateway voice-bubble path all depend on those
extras. Implementations live in a new `regent-speech` crate behind a built-ins-always-win
registry ported from Hermes; one **OpenAI-compatible HTTP adapter** serves every provider
by base URL (see the default-backend note below). (Streaming TTS rides a callback
`AudioSink`, mirroring the existing `DeltaSink`, so the kernel stays futures-free.) **Three flexibility tiers** answer "any model": built-ins, **`command`-type
providers** (a config-declared shell template wrapping any local CLI — Hermes PR-#17843,
wins over a same-name plugin), and registered plugins. The crate also ports Hermes's
hard-won robustness (Whisper hallucination filter, oversized-file chunking, energy VAD).
A model manager downloads/verifies/caches into `$REGENT_HOME/models/` — the explicit,
gated form of `regent-embed`'s auto-download. The feature is **`speech.enabled: false` by
default** (same opt-in shape as `http.enabled`/`board.enabled`); a fresh daemon
loads/downloads nothing. One command, `regent voice setup`, picks providers (defaults
**Qwen3-ASR/Qwen3-TTS**, the speech-capable models — not the dense text model), prompts
for keys via `setup_schema()`, downloads with progress, verifies, and flips the toggle.

**Default backend — Regent downloads the weights, on enable (settled by user decision):**
the defaults are **`qwen3-asr-1.7b` / `qwen3-tts-1.7b`**, and **Regent fetches their weight
files itself** — the tested `ModelManager` (checksum-verified, idempotent, atomic write)
downloads into `$REGENT_HOME/models/<kind>/<model>/` — but **only when voice is enabled**:
`regent voice setup`/`voice enable` call the `voice.ensure_models` RPC, so a fresh,
disabled install downloads nothing. The same OpenAI-compatible wire Hermes uses for Groq/
OpenAI (`{base}/audio/transcriptions` multipart, `{base}/audio/speech`) is reused for one
adapter (`OpenAiCompatAsr`/`OpenAiCompatTts`); a **local runtime serves the downloaded
weights** to it (a bundled server spawned over the weights, or in-process inference — the
**remaining lift**, accepted as such). The *same* adapter serves hosted providers
(`qwen`→DashScope, `groq`, `openai`) by base URL+key with **no download**. Weight sources
(URLs+sha256) are **config-driven** (`speech.asr.weights`/`tts.weights`) — no fabricated
defaults ship; the real Qwen3-1.7B sources are filled in once confirmed. The HTTP call and
the weight downloader are **injected** so `regent-speech` stays network-free and
unit-testable; the daemon supplies the reqwest executor.

**Consequences:** The model is swappable from config and the composition root; the
agent never sees the backend. The `command`-type tier means even an unsupported model is
one config line, never a code change. No multi-GB surprise on install. Remote backends
are the always-available fallback if a local checkpoint is unproven on an OS.
