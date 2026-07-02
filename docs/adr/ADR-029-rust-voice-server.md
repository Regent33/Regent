# ADR-029: regent-voice-server — Rust speech server, secured by construction

**Status:** Accepted (2026-07-02)

**Context:** python-voice-server (FastAPI + faster-whisper + Kokoro/Piper) must move to Rust
(single binary, no venv). Its `/call/turn` reaches the FULL agent with auto-approved tools while
serving `allow_origins=["*"]` — any webpage could drive the user's agent. User mandate: secure.

**Decision:**
- New `regent-voice-server` crate (bin, axum): same endpoints + NDJSON `/call/turn` contract, so
  `ui/` and regent-web work unchanged. Agent brain = `DeaconRpc`, a stdio JSON-RPC client with
  latest-wins interrupts (the CLI's own transport, ported from web_call.py).
- **Security posture:** loopback bind; Host-header allowlist (DNS rebinding); no wildcard CORS —
  explicit regent-web origin only; per-boot token on `/call/turn` (embedded in the served /call
  page, `/call/token` readable only via the CORS grant); assets compiled in; body caps.
  The Python server gets the same origin restrictions until retirement.
- **Engines are ports** (`AsrEngine`/`TtsEngine`) with sherpa-onnx implementations (whisper ASR +
  Kokoro TTS) behind the `local-onnx` feature — **default ON** (user mandate: `regent call` runs
  on Rust + ONNX). Sherpa ships prebuilt libs (`download-binaries`, no cmake), but bindgen must
  run — LLVM/libclang is a build prerequisite (`LIBCLANG_PATH` in `.cargo/config.toml`). Engines
  load in the background; models auto-download on first run (`REGENT_VOICE_AUTODOWNLOAD=0` to
  skip). A `--no-default-features` build still serves, answering 503 + reason.
- Dropped from the port: the daemon-less completions fallback brain, librosa time-stretch, and
  OGG/Opus output (WAV always — Python already fell back to WAV without libsndfile-opus).

**Consequences:** two servers coexist until engine parity is measured; Python is then retired.
regent-web fetches `/call/token` once per page load (cached; "" against Python).
