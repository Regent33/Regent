# Implementation Plan — Voice/Video + Real-Time Vision

Companion to [`proposal/realtime-av-vision-v1.md`](proposal/realtime-av-vision-v1.md).
Atomic-change sequence (one verified commit each), grouped by phase. File paths are
exact; "→ what / why" per the operating loop. Verification command per phase at the
end of each block.

Legend: ➕ new file · ✏️ edit existing · ⚠️ shared-contract touch (additive) ·
✅ built+verified · ⏳ deferred.

**Status (2026-06-22):** V0.1 kernel contracts ✅ · V0.2 crate+registry ✅ ·
V0.3 model manager ✅ · V0.4 remote (OpenAI-compat) + VAD + robustness + WAV ✅
(command/decode/native-local pending) · V0.5 config ✅ · V0.6 `voice.status`/`voice.models`
+ speech factory ✅ · V0.6b reqwest `HttpExecutor` + `make_asr`/`make_tts` + `voice.test` ✅
· V0.7 CLI `regent voice setup|status|models|enable|disable` ✅. **Default is local-first:
`provider: local`, `qwen3-asr`/`qwen3-tts` over a localhost OpenAI-compatible server (no
key, Ollama-style); qwen/groq/openai are config-swappable; native whisper-rs/command
later.** Rust suites green + clippy clean; CLI bun-test + tsc + biome clean. Next: V0.4
command/decode + native local, then V1 (Telegram voice).

---

## V0 — Speech foundation (disabled by default)

**V0.1 — kernel speech contracts**
- ➕ `src/crates/regent-kernel/src/contracts/speech.rs` → `AsrProvider`, `TtsProvider`
  traits (full Hermes-parity surface: `name`, `transcribe`/`synthesize`, `is_available`,
  `list_models`/`list_voices`, `setup_schema`, TTS `stream` + `voice_compatible`) +
  `AudioBuffer`, `AsrOptions`{model,language}, `TtsOptions`{voice,model,speed,format},
  `Transcript`, `ModelInfo`, `VoiceInfo`, `ProviderSetup` types. Why: the
  freely-importable contract layer, beside `EmbeddingProvider`.
- ✏️ `src/crates/regent-kernel/src/contracts/mod.rs` → `pub mod speech;`
- ✏️ `src/crates/regent-kernel/src/lib.rs` → re-export the new types.
- Tests: type round-trips; trait object-safety compile check; default methods compile.

**V0.2 — `regent-speech` crate skeleton + registry**
- ➕ `src/crates/regent-speech/Cargo.toml` ; ✏️ root `Cargo.toml` workspace members.
- ➕ `.../regent-speech/src/lib.rs`
- ➕ `.../regent-speech/src/registry.rs` → built-ins-always-win registry (port of
  `transcription_registry.py` / `tts_registry.py`: name-normalize, reject built-in
  shadowing with a warning, re-register overwrites). Resolution order:
  config `command`-type provider → built-in → registered plugin.
- Tests: built-in shadow rejected; re-register overwrites; case/whitespace match;
  command-type wins over a same-name plugin.

**V0.3 — model manager (download/verify/cache)**
- ➕ `.../regent-speech/src/models.rs` → resolve name → `$REGENT_HOME/models/<kind>/<model>/`;
  HTTPS download with progress callback; checksum verify; idempotent skip-if-present.
  Why: explicit, gated version of `regent-embed`'s fastembed auto-download.
- Tests: path resolution; checksum mismatch rejected; skip-if-valid (fake fixture);
  one `#[ignore]` real-download test (mirror `regent-embed`'s ignored test).

**V0.4 — backends behind the traits**
- ✅ `.../regent-speech/src/infra/remote.rs` → **OpenAI-compatible** ASR+TTS
  (`OpenAiCompatAsr`/`OpenAiCompatTts`): one adapter for OpenAI / Groq / **DashScope-Qwen**
  (the Qwen3-ASR/TTS default), differing only by `base_url`+key — grounded in how Hermes
  serves Groq/OpenAI STT. Request building + response parsing are pure; the HTTP call is
  **injected** (`HttpExecutor`) so the crate is network-free + unit-testable. WAV encode in
  `.../src/wav.rs`.
- ✅ `.../regent-speech/src/vad.rs` → energy VAD + silence-duration auto-stop + peak-RMS
  discard (port `voice_mode.py::AudioRecorder`). [dip-tolerance hysteresis = later refinement]
- ✅ `.../regent-speech/src/robustness.rs` → Whisper hallucination filter + oversized-file
  chunk-range planner (block-aligned, under provider cap).
- ➕ `.../regent-speech/src/infra/command.rs` → `command`-type provider: shell-template
  (placeholders for in/out/text/voice) → any local CLI (whisper.cpp / piper). Hermes PR-#17843.
- ➕ `.../regent-speech/src/decode.rs` → ffmpeg subprocess decode → 16 kHz mono int16 PCM.
- ⏳ local native model — `whisper-rs` (whisper.cpp) ASR + a piper binding — deferred; the
  remote + command paths cover ASR/TTS first (Regent does not hand-roll inference, per ADR-017).
- Tests: ✅ VAD fixture PCM; ✅ hallucination drops "thank you"/repeat/amara; ✅ chunk-split
  rejoins; ✅ OpenAI-compat request shape + mock-executor round-trip; remaining: command
  template expansion, ffmpeg decode (integration).

**V0.5 — config**
- ⚠️ ✏️ `src/crates/regent-deacon/src/domain/config.rs` → add `SpeechConfig` to
  `DaemonConfig` (`enabled:false` default; `asr`/`tts`/`vision` sub-structs;
  `models_dir`; `call.fast_model: ""` for model tiering — ADR-020). Defaults:
  `asr.model: qwen3-asr`, `tts.model: qwen3-tts`. Additive; `deny_unknown_fields`
  honored. Why: opt-in toggle, same shape as `HttpConfig`/`BoardConfig`.
- Tests: default config has `speech.enabled == false`, `asr.model == "qwen3-asr"`,
  `tts.model == "qwen3-tts"`; round-trips through yaml.

**V0.6 — daemon model lifecycle + JSON-RPC**
- ✏️ `src/crates/regent-deacon/src/application/dispatcher/admin_ops.rs` (or new
  `voice_ops.rs`) → `voice.status`, `voice.models`, `voice.ensure_models`
  (streams download progress), `voice.test`. Why: daemon already owns the embeddings
  model on boot; speech models belong there too.
- ✏️ daemon boot → load speech providers only when `speech.enabled` (graceful degrade
  if a model is missing, like embeddings).
- Tests: `voice.status` reflects config; `ensure_models` is idempotent.

**V0.7 — CLI `voice` command group**
- ➕ `src/regent-cli/src/features/voice/cli/voiceCommand.ts` → `setup` (wizard: prompts
  driven by each provider's `setup_schema()` over JSON-RPC, model download progress,
  enable), `enable`/`disable`, `status` (uses `is_available()`), `models`/`voices`,
  `test`. Mirror `setupCommand.ts` (banner/section/ask) and `gatewayCommand` dispatch.
- ✏️ `src/regent-cli/src/app/cli/router.ts` → `case "voice":`.
- ✏️ `src/regent-cli/src/app/config/commands.ts` → add `voice` to a group.
- Tests: arg parse (`voice.test.ts`); status renders disabled state.

**V0 exit:** `cargo test --workspace` + clippy green; `bun test` in `regent-cli`;
`regent voice setup` downloads Qwen3 and `regent voice status` shows enabled.

---

## V1 — Turn-based voice on Telegram

**V1.1 — media envelope (additive, shared contract)** ⚠️
- ✏️ `src/crates/regent-gateway/src/domain/entities.rs` → add `MediaRef { kind, url_or_path, mime, duration_ms }`,
  `MessageEvent.attachments: Vec<MediaRef>` (default empty), `OutboundMessage.media: Option<OutboundMedia>`,
  `OutboundMessage.reply_modality` (text|voice). Derive/extend `Default` so existing
  struct literals get `..Default::default()`; **text-only wire path unchanged**.
- ✏️ update the handful of `MessageEvent`/`OutboundMessage` construction sites.
- Tests: existing gateway tests still pass; new media-envelope round-trip.

**V1.2 — Telegram media parse + transfer**
- ✏️ `src/crates/regent-gateway/src/infra/platforms/telegram.rs` → `parse_updates`
  also emits `voice`/`audio`/`video_note` as attachments (carry `file_id`); add
  `get_file` (file_id → download URL → bytes, **reject > 20 MB Bot-API cap** with a
  clear reply) and `send_voice` (OGG/Opus) / `send_audio` / `send_video_note`. TTS
  output is converted to Opus (ffmpeg) when the provider isn't `voice_compatible`.
- Tests: pure-parse tests on `voice`/`video_note` update fixtures (extend the
  existing `parses_text_updates_and_skips_non_text` test); oversized file → declines.

**V1.3 — runner ASR-in / TTS-out**
- ✏️ `src/crates/regent-gateway/src/application/runner.rs` → before `handle`,
  transcribe `attachments` audio → text (prepend/replace); after `handle`, if inbound
  modality was voice, synthesize the reply and `send_voice` instead of `send`. Inject
  the speech engine behind the kernel traits at the gateway composition root
  (`src/bin/gateway.rs`). Why: keeps `ConversationHandler` text-native (twilio_voice
  pattern, our engine).
- Tests: mock adapter + mock ASR/TTS → voice round-trip; engine-absent → graceful
  "voice not enabled" reply.

**V1.4 — agent-facing tools**
- ➕ `src/crates/regent-tools/src/infra/transcribe_audio.rs`,
  `.../text_to_speech.rs` → two-file tool contract (definition + executor calling the
  speech engine). Why: the agent can also transcribe/synthesize on demand (Hermes
  `transcription_tools`/`tts_tool` parity).
- Tests: executor validates args; returns JSON-string result/error.

**V1 exit:** mock voice round-trip test green; live Telegram voice note → voice reply.

---

## V1b — Local push-to-talk (CLI surface)

**V1b.1 — daemon audio capture/playback**
- ➕ `src/crates/regent-speech/src/audio_device.rs` → mic capture + speaker playback via
  `cpal`; record → VAD auto-stop (V0.4) → WAV; interruptible playback (`stop_playback`
  equivalent). Port `voice_mode.py` semantics, incl. no-device/headless detection that
  degrades gracefully instead of crashing.
- Tests: VAD auto-stop drives a fake capture stream to a stop; headless env reported, not panicked.

**V1b.2 — daemon turn loop + JSON-RPC**
- ✏️ `regent-deacon` → `voice.listen_start`/`voice.listen_stop` (or a streaming method):
  capture → ASR → existing agent turn → TTS → playback, looped until toggled off.
- Tests: loop runs one turn with mock capture + mock ASR/TTS.

**V1b.3 — CLI toggle**
- ✏️ `regent-cli` chat slash command `/voice` + `regent voice mode` → calls the daemon
  toggle; shows a recording/level indicator.
- Tests: command dispatch; indicator renders.

**V1b exit:** `/voice` round-trip on a machine with a mic; no-audio env degrades cleanly.

---

## V2 — Vision (turn-based "see")

**V2.1 — `vision_analyze` tool + routing**
- ➕ `src/crates/regent-tools/src/infra/vision_analyze.rs` → describe an image via the
  configured vision/aux model (text summary); remote URL fetch goes through the existing
  `web_fetch` SSRF guard + a download-size cap (decompression-bomb guard, ~50 MB).
- ➕ `src/crates/regent-providers/src/application/image_routing.rs` → port of
  `decide_image_input_mode` (auto|native|text) + image ref extraction + mime sniff.
- Tests: mode decision table; mime sniff on magic-byte fixtures; SSRF guard rejects
  a private-IP URL; oversize download declined.

**V2.2 — Telegram image parse + runner vision**
- ✏️ `telegram.rs` → parse `photo`/`document`(image) into attachments.
- ✏️ `runner.rs` → in `text` mode, run `vision_analyze` on inbound images / sampled
  `video_note` frames and prepend the summary to the user text.
- Tests: photo-update parse; runner prepends a vision summary (mock vision).

**V2 exit:** send a photo on Telegram → agent describes/acts; `vision.input_mode` honored.

---

## V3 — More platforms
- ✏️ implement `get_file`/`send_voice` (or equivalent) on ≥2 of:
  `discord.rs`, `whatsapp.rs`, `twilio_voice.rs` (swap built-in STT/TTS for ours),
  `slack.rs`. No agent/runner change — they ride the V1 envelope.
- Tests: per-platform pure parse/build tests.

**V3 exit:** ≥2 additional platforms round-trip voice on the shared envelope.

---

## V4 — Real-time calls (separately gated)
- ➕ `src/crates/regent-realtime/` → duplex pipeline: capture → VAD/barge-in →
  streaming ASR → **fast model (quick reply) / main model (thinking) tiering** →
  streaming TTS → playout; real-time frame sampler → vision routing. Transport trait
  with a generic WebRTC impl (`webrtc-rs`/LiveKit) first; ➕ Telegram MTProto
  (`grammers`) voip impl second.
- ➕ tiering router (ADR-020): fast model answers first; on a tool/think signal, play a
  filler clip and re-run on `model.default`. Both providers ride the existing
  `ChatProvider` contract — "any flash model" is config (`speech.call.fast_model`).
- ✏️ `SpeechConfig` → `realtime: { enabled:false, fps_cap, barge_in }`.
- Tests: pipeline state machine (barge-in cancels TTS); tiering escalates on a
  tool-call signal and stays on fast otherwise; transport behind a mock.

**V4 exit:** live call with barge-in; fast model answers and escalates to main on a
think turn; one real-time vision frame reaches the model.

---

## V5 — Native multimodal (optional)
- ⚠️ ✏️ kernel `ChatMessage` → carry image content parts (additive variant).
- ✏️ Anthropic + OpenAI adapters → translate image parts to vendor format; keep image
  parts on the **current user message** so the cached prefix stays byte-stable.
- Tests: image part survives round-trip per provider; prompt-cache prefix unchanged.

**V5 exit:** `vision.input_mode: native` sends pixels; cache-stability test green.

---

## Cross-cutting verification
- Rust: `cargo test --workspace`, `cargo clippy --workspace --all-targets`.
- CLI: `bun test` in `src/regent-cli`, `tsc --noEmit`.
- Each shared-contract change (V1.1, V5) ships its additive-compat test in the same commit.
