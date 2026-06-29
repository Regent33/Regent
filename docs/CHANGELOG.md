# Changelog

## 2026-06-25 — feat(tools): `play` — actually plays a song (not just a search)

- Asking the voice (or CLI) to "play <song>" opened a YouTube **search** page,
  which doesn't play. New core tool **`play`**: resolves the top result with
  yt-dlp and opens the **watch** URL, which plays. Tries `yt-dlp` then
  `python -m yt_dlp` (works without yt-dlp on PATH); falls back to a search if it
  can't resolve. Needs `pip install yt-dlp`. Files: `regent-tools/infra/play.rs`
  (+ registry/mod). Verified: resolves `AC/DC Thunderstruck → watch?v=v2AC41dglnM`,
  daemon builds. Restart so the voice daemon picks up the new tool.

## 2026-06-25 — feat(call): voice can run tool actions; barge-in; no emoji/think aloud

- **Voice tool actions work now.** `control_app`/`terminal` (open an app, run a
  command) are always approval-gated, but the voice/HTTP surface has no way to tap
  "approve" — so it denied. New `AllowAll` approver + `REGENT_AUTO_APPROVE=1`
  (set by the speech server for its dedicated voice daemon; opt out with
  `REGENT_VOICE_AUTO_APPROVE=0`): the spoken command is the consent, so the agent
  can actually "pull up Chrome". Files: `regent-tools` (contracts/lib),
  `regent-daemon/session_manager` (env-gated `approval_handler`), `web_call.py`.
- **Barge-in.** Speaking while Regent talks now cancels the turn (abort the stream
  + stop playback) and starts listening — it no longer talks over you. Echo
  cancellation keeps Regent's own voice out of the detector. `hooks/useCall.ts`.
- **No emoji / `<think>` read aloud.** `_speakable()` strips emoji and reasoning
  blocks from the text before TTS (both brain paths). `web_call.py`.
- Verified: daemon builds + config tests pass, py_compile + web tsc clean, emoji
  strip unit-checked. Restart the speech server to pick up the server side.

## 2026-06-25 — fix(call): make the agent brain actually get used (was silently falling back)

- The call could open apps in `regent chat` but not on the voice call — because the
  voice was **silently falling back to the plain completion brain** (no tools) when
  the agent daemon wasn't up. Now the agent is **warmed at startup** and the console
  states the status up front: `✓ agent brain ready` (voice runs the full agent —
  tools/memory, same as `regent chat`) or `⚠ agent voice off (<reason>)` so the
  fallback is never silent. The unavailable decision is cached (no per-turn spam).
- **Strip `<think>…</think>`** from replies so reasoning models' scratchpad is never
  read aloud. Files: `web_call.py`, `python_server.py`. Restart the speech server to
  pick it up.

## 2026-06-25 — feat(call): agentic voice — the call runs the full agent (tools/memory)

- **The call brain can now be the real Regent agent** (tools, memory, persona),
  not just a chat completion — so "create a kanban task", "what's on my board?",
  "open/download X" actually run, like the CLI. The speech server spawns a
  `regent-daemon` with its HTTP listener enabled (loopback + a random bearer
  token), holds it alive, and POSTs each turn to `/v1/chat` with a persisted
  session; it falls back to the plain completion when no daemon/model is available
  (`REGENT_VOICE_AGENT=0` opts out). The reply still streams to Kokoro per sentence.
  See ADR-025.
- **`REGENT_HTTP_ENABLED/BIND/TOKEN` env overrides** in the daemon config loader so
  `/v1/chat` can be enabled without editing `config.yaml` (loopback + token only).
  `regent voice serve` now passes the profile's `REGENT_HOME` so the agent uses the
  right memory/persona/sessions. Files: `regent-daemon/infra/config_loader.rs`,
  `web_call.py`, `voice/cli/voiceServe.ts`.
- Verified: daemon enables HTTP via env, `/v1/chat` runs a full agent turn (401
  without token; ran the agent with token), 10 config tests pass, CLI + py_compile
  clean. The live tool-loop needs your model key — test with `regent call`.

## 2026-06-25 — fix(call): latency no longer grows as the conversation goes on

- Measured the warm server over 15 sequential turns: **dead flat** (2.43 s →
  2.61 s). So the "gets slower as it grows" was **client-side accumulation**, two
  causes: (1) finished TTS `AudioBufferSourceNode`s were never disconnected, so the
  Web Audio graph grew every turn; (2) the server streamed a `reply` transcript
  update **per token** (~160 re-renders/turn), loading the main thread where the
  (deprecated, main-thread) ScriptProcessor VAD runs — that degrades turn detection
  the longer the call runs. Fixes: `src.disconnect()` on playback end
  (`hooks/useCall.ts`); send `reply` **per sentence**, not per token
  (`web_call.py`). Restart the speech server to pick up the server side.

## 2026-06-25 — feat(call): one-command launch + much lower latency

- **`regent call` is now one command.** It auto-starts the local speech backend
  (detached + reused — no separate `regent voice serve`), **waits for the models to
  warm**, launches the Next UI, and **opens the browser** when it's ready. The cold
  first turn (15–25 s while models load on demand — the real cause of the "5–12 s
  latency") is gone: by the time the page opens, ASR+TTS are warm. Files:
  `call/cli/callServe.ts` (+ `callCommand.ts`), `voice/cli/voiceServe.ts`
  (`speechServerUp`/`speechServerWarm`/`startSpeechServerDetached`),
  `python_server.py` (`/health` now reports `warm`).
- **Streaming brain + sentence-streamed Kokoro TTS.** The turn was: full LLM
  completion → synth whole reply → play. Now the reply **streams**, and each
  sentence is synthesized + sent as it completes, so the voice starts on sentence 1
  while the model writes the rest (Kokoro is ~3× realtime, so chunks stay ahead →
  smooth). Reply tokens capped at 160. Measured warm: ASR ~0.5 s, first audio
  ~1.1 s, full turn ~2.4 s (echo brain; + the model's time-to-first-token live).
  File: `web_call.py` (`_brain_stream`). `[turn]` log now shows `brain_ttft` +
  `first_audio`.

## 2026-06-24 — feat(web): Jarvis call works locally (no LiveKit needed)

- **The Jarvis call UI now does a real call against the local speech server.**
  Before, without a reachable LiveKit room it was a dead-end "local preview only"
  (mic-reactive visualizer, no conversation). `useCall` now falls back to a
  **turn-based local call**: VAD on the mic → POST `/call/turn` on the Python
  server (faster-whisper + Kokoro) → play the streamed reply through the same
  analyser, so the ring reacts to Regent too. LiveKit is still used first when it's
  configured *and* reachable. File: `hooks/useCall.ts`.
- **Live transcript** (what you said + Regent's reply) shown under the ring; new
  `thinking` phase. `components/CallStage.tsx`.
- **`/call` route added** — it 404'd before (the UI is at `/`); now both work,
  matching the URL other surfaces print. `app/call/page.tsx`.
- **CORS** on the Python server so the Next app (`:3000`) can POST to it (`:8000`).
  `python_server.py`. Verified: web `tsc` clean, `py_compile` clean.
  Run both: `regent voice serve` + `regent call serve`, open `http://localhost:3000`.

## 2026-06-24 — fix(cli): `regent voice serve` works from any directory

- It resolved `python-voice-server/python_server.py` as a **cwd-relative** path, so
  it only worked from the repo root (`✗ can't find …` everywhere else). Now it walks
  up from `REGENT_REPO_DIR` / cwd / the binary's dir / the source dir to find the
  repo root (mirrors `callServe`/`findWebDir` and the daemon's locate), and launches
  the server with `cwd = root` so the default `tts-asr-local-models` path still
  resolves. Verified starting from `src/regent-web`. File: `voiceServe.ts`.

## 2026-06-24 — feat(voice): Kokoro-82M TTS (more natural, still real-time)

- **TTS default is now Kokoro-82M** (Piper kept as `REGENT_TTS_ENGINE=piper`).
  Measured on CPU: ~0.65 s synth for a typical reply (~0.4× realtime), 24 kHz —
  much more natural than Piper (~0.1 s but robotic). Per turn ≈ 1.5–2.5 s. Added a
  `_KokoroTTS` adapter (same `.generate_custom_voice()` shape, so the endpoints and
  call streaming are untouched) and `_ensure_kokoro_model()` (downloads the ~340 MB
  model once on first run). `REGENT_KOKORO_VOICE` picks the voice (default
  `af_heart`). `voice serve` now installs/checks `kokoro-onnx`. Files:
  `python_server.py`, `README.md`, `voiceServe.ts`. Verified: synth 0.65 s, 24 kHz.

## 2026-06-24 — perf(voice): real-time speech engine (faster-whisper + Piper)

- **The local voice stack is now real-time.** Measured on an RTX 4060 Laptop, the
  Qwen3-1.7B pair was **~70 s/turn** — both bf16 models are ~8.3 GB and don't fit in
  8 GB VRAM together (CUDA pages to host RAM → thrash), and even TTS-alone-on-GPU
  was ~10 s; ASR fell to CPU at ~58 s. Swapped the engine behind the **same
  endpoints**: **faster-whisper** (CTranslate2 int8) for ASR — **0.2–0.6 s** on the
  GPU — and **Piper** (ONNX) for TTS — **~0.1 s** on CPU. Per turn ≈ **1–2 s**
  (+ the brain LLM). This backend also serves the native `regent call` (its local
  provider POSTs to `/v1/audio/*`).
- **How:** `python_server.py` wraps both engines in adapters preserving the
  `.transcribe()` / `.generate_custom_voice()` interface, so the endpoints, the
  `/call` NDJSON streaming, and `web_call.py` are unchanged. Piper voice
  auto-downloads on first run. `regent voice serve` now checks for
  `faster-whisper`/`piper`/`soundfile` and installs them in one step. Verified
  end-to-end: transcribe 0.37 s, synth 0.07–0.10 s.
- Files: `python-voice-server/python_server.py`, `README.md`,
  `regent-cli/.../voice/cli/voiceServe.ts`. (Also installed the CUDA torch build —
  `2.10.0+cu128` — so the GPU is actually used; the CPU-only torch was the original
  "super latency".)

## 2026-06-24 — feat: real-time calls — LiveKit-Rust transport + a Next.js "Jarvis" call UI

Goal: replace the Python *live-call* path with **LiveKit (Rust)** and ship a
**Next.js** frontend for live calls — a Regent-branded "Jarvis" UI with a
braille-dot voice animation. Lands ADR-021 R2 ("LiveKit + web client"); see
[ADR-024](adr/ADR-024-livekit-rust-transport-and-nextjs-call-frontend.md). The
turn-based `python-voice-server` is untouched (different purpose).

- **feat(web): the Jarvis live-call frontend** (`src/regent-web`, new). Next.js 16 /
  React 19, **Tailwind v4 · three.js (R3F) · React Spring · GSAP** (required stack).
  A glowing teal Regent core ring (three.js) + a **braille-style dot voice
  visualizer** (canvas, audio-reactive) over a HUD gridline backdrop. `livekit-client`
  joins the room + publishes the mic; a server-side token route (`livekit-server-sdk`)
  signs join JWTs from env (self-host **or** LiveKit Cloud). **Always-on**: the call
  auto-starts on load (no button); no LiveKit configured ⇒ graceful local-mic preview.
  Files: `app/`, `components/{CallStage,JarvisRing,BrailleVoiceViz}.tsx`,
  `hooks/useCall.ts`, `app/api/token/route.ts`, configs.
- **feat(realtime): LiveKit/WebRTC transport** (`regent-realtime`). A transport that
  joins a room as the agent, streams the caller's audio into the engine, and
  publishes the engine's audio out (24 kHz mono). **Optional, gated behind the
  `livekit` feature** (native libwebrtc) so the default workspace build is unaffected.
  Files: `src/crates/regent-realtime/{Cargo.toml,src/lib.rs,src/livekit_transport.rs}`.
- **feat(cli): `regent call serve`** — one command: installs web deps on first run,
  seeds `.env.local`, prints the LiveKit/agent bring-up, launches the UI. Files:
  `src/regent-cli/src/features/call/`, `app/cli/{router,help}.ts`, `app/config/commands.ts`.

Verified: web `bun run build` green (`/`, `/api/token`); default `cargo build
--workspace` green; `cargo build/clippy -p regent-realtime --features livekit` green
(**native libwebrtc compiled on Windows**) + 8 engine tests; CLI `tsc` + `biome` clean,
35 tests pass. Live verified the UI in-browser (ring + braille viz react to the mic).
Not done: wiring the transport to a provider in a runnable agent binary (needs a
LiveKit server + Realtime key) — next phase.

## 2026-06-24 — feat(gateway): file-send on webhook platforms (WhatsApp, Slack, WeChat)

Goal: let the agent send files on the webhook platforms (Slack/WhatsApp/Google
Chat/WeChat/Line). Found that webhook platforms had **no agent→platform outbound
path at all** — the daemon's keyed sessions delivered via `NotificationDelivery`
(CLI notifications), so `send_file`/`send_message` never reached the platform.
Built the path, then per-platform uploaders on the same seam:

- **feat: `WebhookFileSender` trait** (gateway). New async trait, separate from the
  pure/sync `WebhookAdapter` so the other adapters are untouched. Files:
  `domain/contracts.rs`, `lib.rs`.
- **feat: per-conversation platform delivery** (daemon). `PlatformDelivery` resolver
  + `WebhookPlatformDelivery`/`WebhookDelivery` sink: a keyed session
  (`platform:chat_id`) now routes the agent's `send_message` **and** `send_file`
  back to the platform's API (replies still go via the webhook handler; local CLI
  sessions unchanged — same `NotificationDelivery`, no file tool). Threaded the
  conversation key through `create/resume_session` (additive `_keyed` variants; no
  `SessionManager::new` signature change). Files: `domain/contracts.rs`,
  `application/session_manager/{mod,build}.rs`, `infra/webhook.rs`,
  `application/http_serve.rs`.
- **feat: uploaders.** WhatsApp (Cloud-API 2-step `/media` → send by id),
  Slack (post-`files.upload` 3-step: getUploadURL → PUT → completeUpload), WeChat
  (temp-media upload → Customer Service `media_id`; image/voice/video only —
  the OA API has no generic document type; caption rides as a preceding text).
  All request/response shapes are pure, unit-tested helpers; only the HTTP calls
  use the injected client. Files: `infra/platforms/{whatsapp,slack,wechat}.rs`.

**Blocked (architectural, not done):**
- **Google Chat** — bot replies *synchronously* in the HTTP response (`SendAuth::None`,
  no outbound token). File upload needs a **service-account OAuth credential + the
  Chat REST API**, which the adapter doesn't carry. Needs new creds + a Chat client.
- **Line** — media messages are **URL-only** (`originalContentUrl`); Line has **no
  file-upload API**, so a local file needs public hosting first (no media-host yet).

Verified: `cargo test -p regent-gateway --lib` (89 pass, +6 new) and `-p
regent-daemon --lib` (33 pass, +2 new) green; `cargo clippy` clean across all
crates. The `daemon_basics` integration binary couldn't relink (a running
`regent-daemon.exe` holds the file) — code compiles; rerun after stopping the
daemon.

## 2026-06-24 — fix(voice): smooth speech (revert per-sentence TTS) + per-turn timing

- **fix: choppy/garbled real-time speech.** Per-sentence streamed TTS synthesized
  each sentence as a separate call and played them as they arrived — but on CPU
  synthesis is **slower than playback**, so multi-second gaps opened between
  sentence chunks (and `Wait... really?` over-split into robotic fragments). Now
  the **whole reply is synthesized in one call** → one smooth utterance with
  natural prosody. The instant `heard`/`reply` text streaming, off-event-loop
  generator, and warm-up all stay. File: `web_call.py`.
- **instrument: per-turn latency log.** Each turn prints
  `[turn] asr=… brain=… tts=… total=… (device)` and sends a trailing `timing`
  NDJSON line — so the real bottleneck is measured, not guessed. (Expectation on
  CPU: TTS dominates → the fix is GPU, see the README + `voice-onnx-feasibility.md`.)

## 2026-06-24 — perf(voice): sentence-streamed TTS (voice starts after sentence 1)

- **perf: `/call/turn` streams.** It was serial — ASR → brain → synthesize the
  **whole** reply → return one audio blob; nothing played until the entire reply
  was synthesized. Now it returns an **NDJSON stream**: `heard` (instant
  transcription), then `reply` text, then **one audio chunk per sentence**, so the
  voice starts after sentence 1 while the rest synthesizes. The generator is sync,
  so Starlette runs ASR/brain/TTS off the event loop (it blocked it before).
  Files: `web_call.py` (+ the standalone `ui/call.html` and the inline fallback —
  client now reads the stream and plays chunks through a queue).
- Verified: `py_compile` clean; `node --check` passes on both pages' JS; the
  sentence splitter unit-checked. The full audio path needs the models running —
  test live with `regent voice serve` → `/call`.

- **perf: background model warm-up.** ASR+TTS lazy-loaded on the first call — a
  10–30 s cold-load cliff on turn one. The server now warms both in a background
  thread at startup (server stays instantly reachable), so the first real call
  skips the load. Added double-checked locking to the loaders so warm-up + a racing
  first request can't double-load multi-GB models. File: `python_server.py`.
- **docs: GPU is the real latency fix.** New `python-voice-server/README.md` — the
  one-command CUDA torch install for the RTX (the server already auto-detects
  `device=cuda:0`), env vars, and the CPU latency notes.
- **docs: Rust/ONNX rewrite assessed — not worth it for speed.** New
  `docs/voice-onnx-feasibility.md`. Evidence: `Qwen3ASRModel`/`Qwen3TTSModel` are
  custom inference wrappers (not `transformers.PreTrainedModel`), `optimum` can't
  export them, and they use bespoke autoregressive decode + a codec/vocoder. The
  bottleneck is 1.7B model compute, not the host language. Recommendation: GPU
  first; int8 quantization (in PyTorch, no export) as the CPU lever.

- **rename:** `scripts/` → `python-voice-server/` (the folder is only the voice
  server); `local_speech_server.py` → `python_server.py`; `static/` → `ui/`.
  `regent voice serve` points at `python-voice-server/python_server.py` (rebuild
  the CLI — the old binary's "can't find …" is from the pre-rename build).
- **fix: the polished call page is now actually served.** `web_call.py` served its
  inline `CALL_HTML`, so edits to the standalone page never showed. `/` and `/call`
  now read `ui/index.html` / `ui/call.html` (inline strings kept as a fallback).
- **fix: call page status never updated.** `state()` wrote to `status.textContent`,
  but bare `status` is `window.status` (a string), not the `#stat` node — a silent
  no-op that made the call look dead. Now references the element.
- **feat: extracted CSS + brand theme.** Styles moved out of the HTML into
  `ui/style.css`; a new `GET /ui/{path}` route serves it + the font (path-traversal
  guarded). Theme is the Regent brand — teal `#00A19B`, cream `#E4DDD3` — with the
  **Kontes compressed-bold** wordmark font bundled at `ui/fonts/` (⚠ personal-use
  licence, see `ui/fonts/LICENSE-kontes.txt` — not for commercial distribution).
- **feat: polished landing.** New `ui/index.html` — ready pill, call CTA, try-TTS
  card. Verified serving via FastAPI TestClient (index, call, `style.css`→text/css,
  font→font/ttf, traversal→404). Files: `python-voice-server/` + `voiceServe.ts`.

## 2026-06-23 — feat(agents): persistent named agents + board execution

- **feat: named-agent registry + CLI** (issue #3). A named agent is a reusable
  definition — name, role, system prompt, optional model + tool allow-list — in a
  new additive `agents` table. Manage with `regent agents list|create|show|edit|
  remove` (width-aware table). Regent's CAPABILITIES + `/agents` slash menu now
  list it. See ADR-023.
- **feat: board runs tasks as the assigned agent.** `kanban assign <task> <agent>`
  now sets the assignee but leaves the task **queued** (`todo`) — `assign_task`,
  not a claim. The board dispatcher claims it (preserving the assignee via
  `COALESCE`) and the runner resolves assignee → that agent's **system prompt** +
  **tool allow-list** (`ToolCatalog::restrict_to`); an unknown assignee falls back
  to the default worker. `model` override is stored, not yet applied at the board
  layer.
- **behavior change:** `kanban assign` no longer auto-moves a task to
  `in_progress` (use `kanban start`). Separates ownership from progress.
- Files: `regent-store` (agents.rs, kanban.rs, entities/schema/lib),
  `regent-tools/catalog.rs`, `regent-agent/board/runner.rs` (+3 tests),
  `regent-daemon` (dispatcher/queries), `regent-cli` (agents + kanban). ADR-023.

## 2026-06-23 — perf(chat): coalesce streaming re-renders (scroll jank)

- **perf: stream deltas flush at ~20fps, not per-token** (issue #5). The chat
  transcript is in Ink `<Static>` (native scrollback), but the live region was
  redrawn on every `message.delta` — per-token redraws thrash the terminal (CPU +
  jank, and you can't stay scrolled up while it's redrawing). `useChat` now buffers
  delta text and flushes on a 50 ms timer; every non-delta event flushes first so
  ordering is preserved. Concatenated deltas reduce to the same state, so the text
  is identical — just fewer frames. File: `features/chat/presentation/useChat.ts`.
  NOTE: this targets the interactive chat's redraw thrash; a one-shot download's
  stderr spinner is a separate surface — see the report for what wasn't reproduced.

## 2026-06-23 — feat(persona): structured user profile + memory routing

- **feat: the `about` profile is now five facets** — identity · preferences · habits
  · constraints · goals (issue #6). Stored as `about.<facet>` persona rows (the
  `persona` table is already KV — no schema change); `persona_block()` renders each
  non-empty facet as a `### Heading`. The bare `about` key stays a back-compat
  catch-all. See ADR-022.
- **feat: CLI CRUD per facet.** `regent about <facet> <show|set|add|edit|clear>` —
  `set` replaces, `add` appends a line, `edit` opens the editor on that facet,
  `clear` empties it. `regent about` shows the whole profile; unknown facets error.
- **feat: memory routing made explicit.** `update_persona` gained a `section` arg
  (target `user`); its description now states what belongs where so the agent stops
  bloating the profile: **profile → the 5 facets (durable only); world/work facts →
  `memory`; what happened → session history; how-to → skills; future intents → cron.**
  This maps the proposal's 7 memory types (§5.3) to the existing subsystems rather
  than duplicating them. Files: `regent-store` (persona.rs, lib.rs),
  `regent-tools/persona_tool.rs`, `regent-daemon` (admin_ops.rs),
  `regent-cli/.../persona/cli/personaCommand.ts`; +3 Rust tests.

## 2026-06-23 — feat(cli): width-aware box tables (kanban · cron)

- **feat: `shared/ui/table.ts`.** A reusable terminal-width-aware box table.
  Columns size to their content; one `flex` column absorbs the leftover width and
  truncates with `…`, so the table never overflows and **re-fits when the terminal
  is resized** (issue #4: "not resolution dynamic"). Cells are sized on their
  *visible* width (ANSI-stripped) then painted, so colour can't break alignment.
  Unit-tested: equal row widths, narrow-terminal truncation, painted-cell
  alignment.
- **feat: kanban · cron · sessions now render real tables** (issues #2, #4).
  `kanban list`, `cron list`, and `sessions list` replaced their hand-rolled
  `padEnd` output with `renderTable` (rounded box, coloured STATUS/STATE). Fixes a
  latent bug where the padded status string never matched the colour map, so
  kanban status was always uncoloured. The flat name+description lists (tools,
  model, skills, memory) are intentionally left as lists — boxing them would be
  heavier than the data warrants. Files: `shared/ui/table.ts` (+ test),
  `features/kanban/cli/kanbanCommand.ts`, `features/cron/cli/cronCommand.ts`,
  `features/sessions/cli/sessionsCommand.ts`.

## 2026-06-23 — fix(cli): cron/daemon commands could hang forever

- **fix: bounded daemon shutdown (the `regent cron …` hang).** Every one-shot
  command spawns its own `regent-daemon` and closed it by sending stdin EOF and
  waiting for `exit` — with **no timeout**. A daemon slow or stuck on boot (first-run
  Windows Defender scan of the freshly-built 60 MB exe, a store-lock, a deadlock)
  meant `close()` never resolved and the CLI hung until an external 60 s SIGKILL,
  silently. `connectDaemon` now force-kills the child after a 2 s grace window, so
  the CLI always exits. Verified against a stub daemon that never responds: prints
  `daemon health check failed` and exits (was: infinite hang). Files:
  `daemon/spawn.ts`.
- **fix: `regent <command> --help` no longer spawns the daemon.** `cron --help`
  fell through to the live `cron.list` path, so it paid the daemon-spawn cost and
  hung alongside it. `--help`/`-h` after any command now prints that command's
  one-line usage locally and exits. Files: `app/cli/router.ts`, `app/cli/help.ts`.

## 2026-06-23 — feat(voice): local Qwen3 speech works · `voice serve` · realtime engine

- **feat: local voice works end-to-end.** `scripts/local_speech_server.py` now runs
  real Qwen3 inference (`qwen_asr` / `qwen_tts`) behind the OpenAI-compatible
  `/v1/audio/*` endpoints — verified producing WAV audio on CPU (speaker `Ryan`,
  English; both configurable via `REGENT_SPEECH_SPEAKER`/`_LANG`/`_DEVICE`). It
  serves a small status + try-TTS page at `/`. Forces UTF-8 stdout so Windows
  cp1252 redirects don't crash it.
- **feat: `regent voice serve`.** One command for the local server: finds Python,
  checks the deps, prints the 2-step install if missing (qwen-asr/qwen-tts pin
  conflicting transformers builds), else launches it. No more manual `python …`.
- **feat: realtime call engine (R0).** New `regent-realtime` crate — a
  transport-agnostic relay (`run_call`) between a call transport and a
  speech-to-speech provider, with tool-call bridging; tests green. See ADR-021.
- **feat: `gateway setup <platform>`.** telegram/discord/whatsapp/slack subcommands
  (bare token stays Telegram). Discord-first realtime calls in progress.

## 2026-06-23 — fix(cli): input wrap · persona edit · voice setup UX · local weights · security

- **fix: multi-line input.** The chat input was a flex row of separate `<Text>`
  spans, so a wrapped line stranded the cursor on row 1. It's now one wrapping
  `<Text>` (caret nested) — long input flows and the cursor tracks across the wrap.
- **fix: persona edit lost your changes.** `regent soul|about edit` shelled out to
  notepad on a temp file; in chat the 30s command timeout killed it (and notepad
  can return before you save), so edits vanished. Now editing is **direct in the
  CLI**: `set "<text>"`, interactive multi-line `edit` (TTY-only), and `clear` —
  no external editor.
- **fix: `voice setup` in chat.** Running `/voice setup` printed a menu it couldn't
  read (subprocess, no TTY) — your keypress went to the chat. It now detects no
  terminal and tells you to run it in a shell or pass `--provider`/`--key`.
- **feat: animated download.** `voice setup`/`enable` show a braille spinner while
  models download (silent when piped). Byte-% progress bar is a follow-up.
- **feat: local weights dir.** ASR/TTS weights default to `./tts-asr-local-models`
  (gitignored, never committed); Qwen3-ASR-1.7B / Qwen3-TTS-1.7B staged there.
- **security: speech key exfiltration.** The speech HTTP executor (daemon + gateway)
  now refuses to send the API key unless the URL is HTTPS or loopback — a plaintext
  or attacker-set `base_url` can't leak the bearer key. Tested.
- **agent behavior.** The system prompt now tells the agent to **trust the exact
  model IDs the user gives** (never claim a current model "doesn't exist") and to
  **never shell out to the `regent` CLI on itself** (that recursion deadlocked).
- **docs.** Corrected: Qwen3-ASR/TTS-1.7B are real open-weight models (run via vLLM,
  which downloads + serves them; Regent points at the server). Dev setup guide.

## 2026-06-23 — feat(voice): turn-based Telegram voice + speech stack · `/` command menu · dev docs

- **Speech stack (`regent-speech`), disabled by default.** Kernel `AsrProvider`/
  `TtsProvider` contracts (with `transcribe_file` for passing encoded audio
  straight to a Whisper-style endpoint — no PCM/ffmpeg); an OpenAI-compatible
  HTTP backend (one adapter, many base URLs — local/groq/openai/dashscope);
  built-ins-always-win registry, VAD, hallucination filter, WAV, model manager.
  `SpeechConfig` in the daemon (off by default; defaults **qwen3-asr-1.7b /
  qwen3-tts-1.7b**; per-model `weights`). Daemon RPC `voice.status` /
  `voice.models` / `voice.ensure_models` / `voice.test`; CLI `regent voice
  setup|enable|disable|status|models` with **download-on-enable**.
- **Turn-based voice on Telegram (V1).** A voice note is downloaded via `getFile`
  (20 MB cap) and transcribed into a normal text turn; if the chat last spoke, the
  reply is synthesized to Opus and sent back with `sendVoice` (graceful text
  fallback). Self-contained in the Telegram adapter — `MessageEvent` and the
  runner stay text-only. Opt-in via `REGENT_SPEECH_BASE_URL` (+ key) on the
  gateway; the adapter is split into `telegram.rs` / `telegram/voice.rs` /
  `telegram/wire.rs` (each ≤ 200 lines).
- **Security on the weight downloader.** `ModelManager` rejects path-traversal in
  model `id`/file `name` before any write; weight URLs must be HTTPS (loopback
  exempt) with an 8 GiB size cap.
- **`/` command autocomplete menu (Claude-Code-style).** Typing `/` in chat opens
  a filtered, keyboard-navigable picker (↑↓ select · ⇥ complete · ↵ run · esc
  dismiss) with command descriptions; `voice` added to the Commands list.
- **Developer setup docs.** New [`docs/development/`](development/README.md):
  building the Rust core (cargo), the TypeScript CLI (bun), and how voice/API
  calls are configured and made — including the fix for "regent-daemon not found"
  (`cargo build -p regent-daemon`).
- **Design docs.** Proposal + phased plan + ADRs 016–020 for voice/video calls
  and real-time vision (turn-based first, real-time later; Regent-downloaded
  weights on enable; vision routing; call model tiering).

## 2026-06-22 — feat(cli): surface working backend subcommands (kanban start/review + help)

- **`kanban start` / `kanban review`.** The board's status flow is
  `todo → in_progress → in_review → done` (with `blocked` reachable from anywhere) — the
  columns the agent's own kanban tool already drives. The daemon's `kanban.set_status`
  accepts any of them and `set_task_status` writes unconditionally, but the CLI only exposed
  `block`/`unblock`/`complete`. Added the two missing column moves — `kanban start <id>`
  (→ `in_progress`) and `kanban review <id>` (→ `in_review`) — reusing the existing
  `setStatus` helper (a constant pass-through to the already-tested RPC; no backend change).
- **`regent help` now lists the working subcommands it had been hiding.** The one-liners for
  `cron` (now shows `pause · resume · run · edit`, shipped 2026-06-18) and `memory`
  (`list · pin · unpin · forget`, same) understated commands that already work; `kanban`
  gains `start · review`. Pure help-text — these subcommands were callable already, just
  undocumented in the CLI surface.

**Verified:** `tsc --noEmit` clean · `biome check` clean · `bun test` 30/30 green. No live
daemon smoke (no prebuilt binary; avoided a `cargo build` that would collide with a
concurrent voice-stack session). **Not touched:** the `voice` subsystem — another session is
actively building it; the one remaining unwired daemon method (`voice.test`) is theirs.

## 2026-06-22 — docs: plan for voice/video calls + real-time vision

- **Proposal + phased plan for voice/video calls and real-time vision.** Full design
  in [`docs/proposal/realtime-av-vision-v1.md`](proposal/realtime-av-vision-v1.md) with
  an atomic-change sequence in
  [`docs/realtime-av-implementation-plan.md`](realtime-av-implementation-plan.md). Key
  decisions: (1) **turn-based first, real-time later** — voice *messages* ride the
  Telegram Bot API today (the `twilio_voice` turn shape); true duplex calls are isolated
  in a later `regent-realtime` crate (WebRTC, then Telegram MTProto). (2) A **pluggable
  speech stack** (`regent-speech`) with `AsrProvider`/`TtsProvider` kernel traits and a
  built-ins-always-win registry (Hermes parity), **disabled by default**, enabled by one
  command `regent voice setup` that downloads models with progress (the explicit form of
  `regent-embed`'s auto-download). Defaults: **Qwen3-ASR + Qwen3-TTS**, swappable to any
  model via config. (3) **Vision routing** ported from Hermes (`text` mode first, native
  multimodal later). (4) **Call model tiering** — a fast model (e.g. Gemini 3.1 Flash
  Lite) answers quick spoken turns, escalating to the main model for thinking. Media
  flows through an **additive** `MessageEvent.attachments` envelope (text path
  unchanged). Revised after a deeper read of Hermes's actual implementation, which added:
  a second **local CLI push-to-talk** surface (daemon mic/speaker via `cpal`, `/voice`),
  a richer provider contract (`is_available`/`list_models`/`setup_schema`/streaming/
  `voice_compatible`), **`command`-type providers** (wrap any CLI via a shell template),
  and a robustness layer (Whisper hallucination filter, oversized-file chunking, energy
  VAD, 20 MB Telegram `getFile` cap, OGG/Opus voice notes). ADRs:
  [016](adr/ADR-016-media-capable-gateway-envelope.md) (envelope),
  [017](adr/ADR-017-pluggable-speech-stack-disabled-by-default.md) (speech stack),
  [018](adr/ADR-018-realtime-calls-transport.md) (calls transport),
  [019](adr/ADR-019-vision-routing.md) (vision), and
  [020](adr/ADR-020-call-model-tiering.md) (model tiering). **Plan only — no code yet;
  awaiting "go".**

## 2026-06-22 — feat: app control · thinking indicator on chat platforms

- **`control_app` — desktop/app automation (approval-gated).** Runs an OS automation script
  (PowerShell incl. UI Automation/SendKeys on Windows, AppleScript on macOS, shell on Linux) to
  focus windows, send keystrokes, script menus. **Every call is approval-gated** through the
  surface's handler (CLI prompt / Telegram `/approve`), so an unattended or denied call never runs;
  120s timeout + output cap. In the core catalog (CLI + gateway). (Browser control — cloud
  Browserbase over CDP — is the next chunk.)
- **"thinking" indicator on messaging platforms.** While a turn runs, the gateway now refreshes the
  platform's native typing indicator (Telegram `sendChatAction`, Discord `/typing`) every 4s, so
  the user sees the agent working the whole time — stopping the moment the reply is sent. Added
  `PlatformAdapter::send_typing` (default no-op).
- **browser control via Playwright MCP (opt-in, approval-gated).** The same mechanism Claude Code
  uses: point `REGENT_BROWSER_MCP_URL` at a running Playwright(-compatible) MCP server and the
  agent gains its browser tools (navigate / snapshot / screenshot / click / type / …). Set-up:
  `npx @playwright/mcp@latest --port 8931` then
  `regent keys set REGENT_BROWSER_MCP_URL http://127.0.0.1:8931/sse`. **Mutating actions** (click /
  type / fill / submit / press-key / evaluate / upload) are **approval-gated**; read/navigate run
  freely. Attachment is best-effort (a down server logs a warning, never breaks a turn) and
  per-session in both the CLI daemon and the gateway. Chosen over a bespoke Browserbase/CDP client:
  free, local, private, and reuses Regent's existing MCP client.
- **web search + fetch (#1).** Pluggable `web_search` across six providers; Regent auto-selects a
  provider when its key is present, floors results at 12 sources, and the agent must finish with a
  cited `References` list. `web_fetch` reads a known URL with an SSRF guard.
- **send files to platforms (#7).** `send_file` tool delivers a file to the current chat (Telegram
  `sendDocument`, etc.) via the platform adapter.
- **`regent keys` + `manage_keys`.** Manage provider API keys in `$REGENT_HOME/.env` from the CLI
  (`keys list | set | rm`); the agent saves a pasted key with the `manage_keys` tool (masked, never
  echoed) instead of refusing.
- **durable preferences now reach soul/about.** The post-turn review fork can update the persona,
  so "always be concise" (→ soul) and durable user facts (→ about) actually persist and show in
  `regent persona`, not only in graph memory.
- **model-agnostic prompt.** The agent no longer asserts a specific underlying model, version,
  training data, or knowledge-cutoff (it was inventing "MiniMax-M3, cutoff Jan 2026"). Design-
  lineage references were also removed from source comments and the review prompt.
- **command self-knowledge.** A `CAPABILITIES` reference (the full CLI command surface + how to
  invoke it, in terminal and via `/<command>` in chat) is injected into the daemon and gateway
  system prompts, so the agent describes what it can do accurately instead of inventing commands.
- **chat platforms get plain text.** Replies over Telegram/Discord/etc. are flattened from markdown
  to readable plain text at the gateway (`**bold**`→bold, pipe tables→spaced rows,
  `[text](url)`→`text (url)`, headings/bullets/fences de-marked); the CLI still renders rich
  markdown.
- **fix: duplicated final answer.** The TUI could render a reply twice (a mid-turn-committed partial
  plus the authoritative `message.complete` reply); it now commits the reply once and collapses a
  superseded partial within the same turn.
- **fix: browser URL sanitize.** Malformed `url` args to the browser tool (a stray leading quote or
  a dropped scheme colon, `"https//…"`) are repaired before navigating.
- **fix: Windows terminal quoting.** `cmd /C <command>` was mangling quoted commands — Rust's `\"`
  escaping (which cmd.exe doesn't grok) turned `start "" "https://…"` into an attempt to open `\\`.
  The command line is now passed to cmd verbatim via `raw_arg`, fixing browser/app launches and any
  quoted command on Windows.

## 2026-06-21 — feat: persona-in-DB + agent self-editing · learning-loop fixes · chat UX

- **persona moved to the DB.** `soul` (agent identity) + `about` (user profile) live in the
  `persona` table (no plaintext files); legacy `soul.md`/`about-you.md` are imported then deleted.
  View both at once with `regent persona` (or `/persona`); edit via `regent soul|about set|edit`
  (terminal) or `/soul`, `/about` (chat).
- **the agent can edit its own persona + your profile.** New `update_persona` tool (set/append/get,
  target self/user) — registered in the daemon + gateway. The base prompt also directs the agent to
  *proactively* record durable user preferences to `about` as it works.
- **model-agnostic prompt.** The base prompt no longer lets the model invent its underlying model,
  version, training data, or knowledge-cutoff (it was claiming "MiniMax-M3, cutoff Jan 2026").
- **learning loops (vs Hermes).** The skill **curator now auto-runs** (6h background pass; stale
  agent-created skills → archived, pinned/user exempt). The post-turn **review fork also fires on a
  partial-failure** turn (interrupted mid-tool), not only on success. See
  `docs/learning-loops-gaps.md`.
- **chat UX.** Prompts typed while a turn is busy are **queued** (FIFO) and sent when it finishes,
  instead of being silently dropped; user messages + AI replies get a blank line of breathing room.
- **help.** `/help` + the welcome panel now note that any command also runs in chat with a `/`
  prefix (e.g. `/status`, `/kanban list`, `/soul`).
- **open apps/files (#3).** The terminal tool's description is now OS-aware and names the launcher
  (Windows `start`, macOS `open`, Linux `xdg-open`) with examples, so "open chrome" / "open this
  file" actually launches — the mechanism already worked via `cmd /C`, the agent just didn't know.
- **per-object artifacts (#6).** Generated standalone artifacts/projects each get a dedicated folder
  under `<REGENT_HOME>/artifacts/<slug>/` (distinct from edits to your existing files); the daemon +
  gateway prompts carry the directive and the base `artifacts/` dir is created at boot.
- **live web search + fetch (#1).** New `web_search` and `web_fetch` tools (in the core catalog, so
  both CLI and gateway have them). Pluggable providers mirroring the gateway platform adapters —
  **Brave, Tavily, SerpAPI, Exa, Google CSE**, and **DuckDuckGo (keyless, the default)** — selected
  by `REGENT_SEARCH_PROVIDER`; key from `REGENT_SEARCH_API_KEY` or the provider's own env
  (`BRAVE_API_KEY`, `TAVILY_API_KEY`, `SERPAPI_API_KEY`, `EXA_API_KEY`, `GOOGLE_CSE_API_KEY`+`GOOGLE_CSE_CX`).
  Each provider's request-build + response-parse is pure and unit-tested.
  - **security (SSRF hardening, reviewed via secure-code-guardian).** `web_fetch` resolves the
    target host and **refuses non-public addresses** (loopback, private, link-local incl. the
    `169.254.169.254` cloud-metadata IP, ULA, CGNAT); redirects are followed manually so **every
    hop is re-validated** (no redirect-based bypass); the body is read under a **5 MB cap** (memory
    DoS); only `http(s)` is allowed. Disable either tool via `tools disable web_search|web_fetch`.
- **send files to platforms (#7).** New `send_file` tool: the agent can upload a generated file to
  the user's chat. Implemented for both polling adapters — Telegram (`sendDocument`) and Discord
  (multipart) — via a new `PlatformAdapter::send_file` (default "unsupported"). **Security:** the
  path is canonicalized and confined to the working dir or `<REGENT_HOME>/artifacts`, and
  secret-ish files (`.env`, `*.db`, `*.key`, `*.pem`) are blocked (exfiltration guard). The 16
  webhook platforms (text-only builder) are a follow-up.
- **provider key management.** New `regent keys` — `list` (masked status of search + platform
  keys), `set <NAME> <value>` (upsert: adds if missing, updates if present), `rm <NAME>` — editing
  `$REGENT_HOME/.env`. The AI-model key (`REGENT_API_KEY`) is protected (managed by `regent setup`).
  Changes apply on the next chat / gateway start.
- **search auto-selects a keyed provider.** With no explicit `REGENT_SEARCH_PROVIDER`, `web_search`
  now picks the first keyed provider whose key is present (Brave → Tavily → SerpAPI → Exa →
  Google CSE), falling back to keyless DuckDuckGo. So `regent keys set TAVILY_API_KEY …` (or pasting
  the key in chat) is enough to get real ranked results — no separate provider step needed.
- **search policy: ≥12 sources + always cite references.** `web_search` now floors the result
  count at **12** (max 20) at the tool level, so every search pulls at least a dozen sources
  regardless of what the model asks. The base prompt + tool description require the agent to **cite
  its sources** — finish web-based answers with a numbered References list of the links used, and
  never present web facts without references. (Google CSE caps at 10/request — a provider limit;
  the other keyed providers honor 12.)
- **the agent can save keys you paste.** New `manage_keys` agent tool (set/list/delete) — when you
  give the agent a provider key in chat, it stores it to `.env` and confirms with a **masked** value
  (the full key is never echoed back), instead of refusing. The base prompt now treats saving the
  user's own provider keys as expected. Protected/runtime vars (`REGENT_API_KEY`, `REGENT_MODEL`, …)
  are not writable through it.

## 2026-06-20 — feat: in-chat commands · full markdown · kanban table

- **in-chat commands**: any `/<command> [subcommand]` (and `regent <command>` typed in chat) runs
  the real CLI as a subprocess and shows its output; chat-native ones (`/help /doctor /new /stop
  /approve /deny /quit`) stay local. Interactive/long-running commands (setup, edit, `-f`, mcp,
  chat) are guided to a terminal.
- **markdown rendering**: assistant output now renders inline `**bold**`, `*italic*`, `` `code` ``,
  headings, and bullet/numbered lists (plus the existing aligned tables) instead of raw markup.
- **kanban list**: renders as an aligned ID · STATUS · ASSIGNEE · TITLE table in the CLI.
- **build note**: the daemon locate prefers `target/release`; rebuilt the release `regent-daemon`
  so kanban/transcript-recovery/persona reach the binary `regent` actually runs.

## 2026-06-20 — fix: gateway env · feat: persona, thinking/table rendering, interrupt recovery

- **gateway start (Telegram)**: the gateway fataled with `REGENT_MODEL not set` and
  immediately died, so `status` showed "not running". The CLI now surfaces `REGENT_MODEL`/
  `REGENT_PROVIDER`/`REGENT_BASE_URL` from `config.yaml` into the gateway's env, and validates
  `REGENT_TELEGRAM_TOKEN` + `REGENT_API_KEY` + `REGENT_MODEL` up-front (clear "missing
  configuration" message instead of a silent crash). Verified: gateway now logs
  "regent-gateway (telegram) up".
- **persona**: `regent soul` / `regent about` edit `$REGENT_HOME/soul.md` (agent persona) +
  `about-you.md` (user profile); the daemon injects both into the system prompt.
- **chat rendering**: `<think>…</think>` → dim/italic "✻ Thinking" (Claude-Code style);
  markdown tables rendered aligned + ruled.
- **interrupt recovery**: an interrupt mid-tool-dispatch is settled with synthetic tool
  results (persisted) so the next message / a resume stays legal.
- **daemon locate**: `regent` finds `regent-daemon` from any directory (walks up from the CLI
  binary's own location, not just cwd) + the `regent` PATH shim (see QUICKSTART).

## 2026-06-20 — chore: retire the Go CLI · rename regent-tui → regent-cli · git baseline

- **Go CLI retired.** The legacy Go CLI at `src/regent-cli/` (cobra) is removed. The TypeScript/Ink
  front-end is now the **sole** CLI plane — superseding ADR-012, resolving ADR-014's "coexist, don't
  replace" decision. (Earlier CHANGELOG entries call the front-end `regent-tui`; that is now
  `regent-cli`.)
- **Renamed `src/regent-tui` → `src/regent-cli`.** Package `name`/`bin` (`regent` → `dist/regent-cli`),
  the compile output (`dist/regent-cli`), CI (the `go` job replaced by a Bun `cli` job: typecheck ·
  lint · test · compile), and ADR-012/014 + the parity plan updated. Builds + 20 tests green from the
  new path; `dist/regent-cli.exe --version` → `regent 0.1.0`.
- **Git initialised.** First `git init` for the repo: a baseline commit on `main` (the Go CLI is
  preserved in that commit before removal, so the retirement is reversible), then this rename on top.
  `.gitignore` excludes build output, deps, secrets (`.env`), and local data (`*.db`).

## 2026-06-19 — feat: insights + transcript-recovery fix + setup wizard + welcome-panel redesign

- **`regent insights`** (B4.3) — usage rollup across every session: sessions, messages, turns
  (ok/failed), api calls, and token spend. New `Store::insights()` aggregate (one read over `sessions`
  + the `turns` ledger), surfaced via `SessionManager::insights` → daemon `insights.get` → CLI. No
  stubs; store unit test + the 21 daemon tests stay green.
- **`regent debug`** (B4.4) — assembles a redacted bug-report bundle under `$REGENT_HOME/debug/`:
  system info, a secret-stripped copy of `config.yaml` (keys/tokens/passwords masked), and the latest
  daemon logs. `.env` (API keys) and `state.db` (conversation history) are deliberately excluded, with
  a README listing what's in/out. Pure CLI — no daemon round-trip. (`security audit` already shipped.)
- **Transcript recovery.** A failed/interrupted turn no longer leaves a dangling user message that
  trips the "two user messages in a row" invariant on the next turn — `Transcript::drop_trailing_user`
  trims it from the in-memory transcript (the store keeps the row). Unit-tested; the mid-call-interrupt
  test still asserts the store keeps exactly the user row.
- **`regent setup` rewrite.** Switched off `node:readline` (which stalled on sequential questions under
  Bun) to Bun's synchronous `prompt()`. Reworked into a Hermes-style wizard: boxed banner → "Model &
  Provider" section → prompts with defaults + descriptions → ✓ completion summary with next steps.
- **Welcome panel redesign.** Categorised **Skills / Tools / Commands** (Hermes-style `category: a, b`),
  with the king mark on the right and model + working directory + session centred beneath it. Wordmark
  reworked into a 3D-extruded block font (bright top-left rim, dark bottom-right depth). Full-width
  panel + framed input; the king is pinned so the text column can't distort it.
- **Quieter startup.** `info` logs (e.g. bootstrap) are gated behind `REGENT_LOG`, so the interactive
  CLI opens clean; dev (`bun run dev`) clears Bun's `$ …` echo (`3J`/`2J`/home).

**Verified:** `cargo test -p regent-store -p regent-daemon` + `clippy -D warnings` green · `tsc` +
`biome` + `bun test` (20) clean · `bun build --compile` ok · live `regent insights` smoke.

## 2026-06-19 — feat/fix: regent-tui — exact king logo from PNG, teal wordmark, Ctrl-C fix

- **Exact king logo from the PNG.** New dev tool `scripts/png-to-terminal-art.ts` rasterises a PNG into
  half-block cells (truecolor `▀` fg/bg, alpha-trimmed, aspect-preserved) and emits a generated TS data
  module (`kingArt.generated.ts`) — so the binary carries only the cell data, no image decoder
  (`pngjs` is a dev-only dep). The welcome panel renders the real `assets/regent-king.png` (gold crown,
  silver body) via a shared `PixelArt` component + `ArtCell` type. Sized to 20 cols (panel auto-fits).
- **Wordmark.** "REGENT" is now a bold, **outlined** pixel font (teal-gradient fill + bright-teal
  outline ring — the HERMES-AGENT display look), rendered through the same `ArtCell`/`PixelArt` path.
  The panel outline is teal too. The dead hand-drawn king/canvas code in `art.ts` is removed (the king
  is the PNG).
- **Ctrl-C fixed.** `render(…, { exitOnCtrlC: false })` so the chat's interrupt-then-double-tap-to-exit
  handler runs — Ink was quitting on the first press before our handler.
- **security audit** — a security-focused companion to `doctor`: checks `$REGENT_HOME`, that a provider
  key is present, and lints `config.yaml` for secret-looking values that belong in `.env`. Pure CLI.

**Verified:** `tsc` + `biome` + `bun test` (20) clean · `bun build --compile` ok · render smoke shows
the PNG king + teal REGENT in the titled panel.

## 2026-06-18 — feat: CLI parity B2 (partial) — gateway control + auth; Ctrl-C double-tap

- **gateway setup/start/stop/status** — manage the separate `regent-gateway` process from the CLI: a
  PID file under `$REGENT_HOME`, secrets in `.env`, logs to `logs/gateway.log` (mirrors how `mcp serve`
  spawns `regent-mcp`). No daemon round-trip — the gateway has no IPC surface (see ADR-015).
- **auth status/revoke** — read/edit the gateway's `gateway-auth.json` (allow_all · operators ·
  paired). Pure filesystem.
- **Ctrl-C double-tap** — in chat, Ctrl-C interrupts a running turn; a second press within 1.5s exits
  (with a "press Ctrl-C again to exit" hint), so a single press never quits by accident.
- **Deferred (later B2 increment):** interactive pairing/`login` (codes issued over chat by a running
  gateway) and message delivery (`send` + per-platform adapter config) — both need the live gateway.

**Verified:** `tsc` + `biome` + `bun test` (20) clean · `bun build --compile` ok · live smokes
(isolated profile): gateway status→setup(.env written)→stop; auth status / revoke. No daemon change.

## 2026-06-18 — fix: regent-tui input — Backspace works after history recall

The message input split Backspace (delete-before-cursor) from Delete (delete-at-cursor), but terminals
disagree on which flag the Backspace key sets — after recalling a history entry (cursor at end-of-line),
Backspace hit the delete-at-cursor branch and no-op'd. Now both keys delete before the cursor (the
standard Ink-input behavior). **Verified:** `tsc`/`biome`/`bun test` clean · `bun build --compile` ok.

## 2026-06-18 — feat: CLI parity B1 (cron, memory, skills, tools lifecycle)

- **cron pause/resume/run/edit** — daemon `cron.set_enabled` (re-enable recomputes `next_run_at`),
  `cron.run` (mark due now → the next scheduler tick runs it), `cron.edit` (name/schedule/prompt).
  Pure dispatcher work over the existing `regent-cron` repo. CLI: `cron pause|resume|run|edit`.
- **memory list/pin/unpin/forget** — `regent-store` gains `set_node_ttl` (pin = clear the TTL → exempt
  from the purge loop) + `recent_nodes`; `regent-graph` gains `pin/unpin/forget/recent_nodes` (+ the
  `MemoryNode` type). Daemon `memory.list/pin/unpin/forget`. CLI: `memory list|pin|unpin|forget`
  (📌 marks pinned). `restore` is deferred — there's no archive backend to restore from (honest, not
  stubbed).
- **skills view/create/opt-out** — daemon `skills.view/create/opt_out` over the existing
  `SkillLibrary` (`view`/`create`/`archive`); skill descriptions keep the domain validation (1–60 chars,
  end with a period). CLI: `skills view <name>`, `skills create <name> --description <d> (--body | --file)`,
  `skills opt-out <name>`. Hub `install` deferred (network/agentskills.io integration).
- **tools list/enable/disable** — `ToolCatalog::disable` filters tools by name; a new `ToolsConfig`
  (`tools.disabled` in config.yaml) is threaded through `SessionManager` and applied to every session
  catalog (the model never sees disabled tools). Daemon `tools.list` (catalog + per-tool enabled flag).
  CLI: `tools list` (● enabled / ○ disabled), `tools enable|disable <tool>` (edits config.yaml).

**Verified:** `cargo build` + `clippy -D warnings` clean across store/graph/tools/daemon · `cargo test
-p regent-daemon` 21 pass (fixed the `SessionManager::new` test call site for the new arg) · `tsc` +
`biome` + `bun test` (20) clean · `bun build --compile` ok · live smokes (isolated profiles): cron
add→pause→resume→edit→run→list; memory list/pin/forget; skills create→list→view→opt-out; tools
list→disable→(○)→enable round-trip.

## 2026-06-18 — feat: CLI parity B0 — status, profile, config set, sessions resume

First batch of the [CLI parity plan](cli-command-parity-plan.md). Real logic, no stubs.

- **`status`** — new daemon method `status.get` (+ `version`) returning active model, live in-memory
  session count, and a cron summary (jobs/enabled/next run). New `SessionManager::active_sessions`.
  CLI prints a compact status block.
- **`profile list|create|delete`** — manage `~/.regent-profiles/<name>` homes (filesystem; no daemon).
  `delete` requires `--force` (a profile home holds `state.db` + `.env`).
- **`config set <key> <value>`** — edits `$REGENT_HOME/config.yaml` in place (dotted key path, atomic
  write, value coercion) via the `yaml` package; takes effect next run (the CLI spawns a fresh daemon
  that reloads config). `config get` unchanged.
- **`sessions resume <id>`** — opens the chat surface on an existing session: `useBootstrap` calls the
  existing `session.resume` instead of `session.create` when given an id.
- **tsconfig:** dropped `baseUrl` (TS 5 resolves `paths` relative to the config dir) — clears an
  editor error; aliases still resolve under `tsc`, `bun test`, and `bun build`.

**Verified:** daemon `cargo build` + `clippy -D warnings` clean · `cargo test -p regent-daemon` 21
pass · `bun test` 20 pass · `tsc` + `biome` clean · `bun build --compile` ok · live smokes vs the
daemon: `status` (model/sessions/cron), `profile` create/list/delete, `config set`→`config get`
round-trip under an isolated profile.

## 2026-06-18 — fix: regent-tui brand — wordmark back to silver gradient (panel-width) + silver #E4DDD3

- Reverted the REGENT wordmark from the 3D gold experiment to the flat silver-gradient half-block style
  (the ADR-012/ADR-014 original) and tightened the letter gap to 1px → 65 cols, the same width as the
  welcome panel below it (no longer overflows).
- Brand silver is now **#E4DDD3** (warm off-white); the silver gradient ramp is re-anchored on it. Teal
  #00A19B accent and the gold crown are unchanged.

**Verified:** `tsc --noEmit` clean · `biome check` clean · `bun test` 20 pass · `bun build --compile` ok.

## 2026-06-18 — feat: regent-tui Phase 4 (polish) — input editing/history + titled panel border

- **Input editing:** the message input is now a real single-line editor — ←/→ move the cursor,
  Backspace/Delete edit around it, printable keys insert at the cursor, and ↑/↓ recall submitted
  prompts (command history; beyond Go's textinput, which had none). The caret is an inverse block
  rendered at the cursor position. (`MessageInput`.)
- **Panel title in the border:** the panel now sets its title into the top rounded border
  (`╭─ Regent v0.1.0 ───╮`) — the Go look. Ink can't title a border, so the top edge is drawn by hand
  and the body box uses every edge but the top at a shared, content-hugging width. `WelcomePanel`
  computes the width from its content (king column + widest info line); the error panel from its text.

**Verified:** `bun test` 20 pass · `tsc --noEmit` clean · `biome check` clean · `bun build --compile`
ok · render smoke: the welcome panel shows the title set into the border with aligned corners, and the
input renders the block caret.

## 2026-06-18 — feat: regent-tui Phase 3 — Go-parity subcommands + command router

Bare `regent` / `regent chat` still open the Ink TUI; everything else is now a one-shot command
(call daemon → print → exit), mirroring the Go CLI's surface.

- **Router** (`app/cli/`): `extractProfile` pulls the global `-p/--profile`; the first positional
  dispatches. `withClient` spawns the daemon, health-checks, runs the handler, and always closes (the
  Go `withClient` pattern). A small tested `parseFlags` covers `--name v`, `--name=v`, booleans, and
  short aliases. One-shot output uses an ANSI `style` helper (auto-disabled off-TTY / `NO_COLOR`).
- **Commands** (one cohesive handler per feature, mirroring the Go files): `model [list|set]`,
  `skills`, `config`, `sessions list|search`, `cron list|add|remove`, `memory pending|approve|reject`,
  `logs [-f]`, `doctor`, `mcp serve`, `setup`, `version`, `help`.
- The chat render path moved to `app/cli/runChat.tsx`; `main.tsx` is now just `runCli(argv)`.

Files: `app/cli/{args,runtime,help,router,runChat}` + `features/{inspect,sessions,cron,memory,logs,
doctor,mcp,setup}/cli/*`; `main.tsx`; test `app/cli/args.test.ts`.

**Verified:** `bun test` 20 pass · `tsc --noEmit` clean · `biome check` clean · `bun build --compile`
ok · **live subcommand smokes against the real daemon**: `version`, `help`, `doctor` (all checks
passed), `model` (claude-sonnet-4-6), `skills`, `sessions list` (real rows), `cron list`, unknown
command (→ help, exit 1).

## 2026-06-18 — feat: regent-tui — 3D gold REGENT wordmark + blinking input caret

- **Wordmark:** rebuilt as a 3D extruded gold pixel font (gold face gradient + dark-amber down-right
  drop shadow), rendered with per-pixel fg/bg via the half-block ▀ two-colour trick — matching the
  reference banner's bold look. Colours are constants (`FACE_RAMP`/`SHADOW`) for a one-line revert to
  silver. Updates ADR-012's "silver REGENT" per user direction.
- **Input caret:** the message input draws its own blinking block caret (Ink hides the hardware
  cursor), so there's a visible cursor when typing.

**Verified:** `tsc --noEmit` clean · `biome check` clean · `bun build --compile` ok · render smoke
shows the extruded wordmark and the `❯ █` caret.

## 2026-06-18 — fix: regent-tui — bold solid king mark with a gold crenellated crown

The kneeling-king mark rendered faint at terminal size (braille dots). Switched it to SOLID
half-blocks via a 2:1 downsample (`packSolid`) so it reads as a bold filled sprite like the wordmark,
and redrew the crown with three even-aligned 2px merlons (2px gaps) that survive the downsample — so
the gold crenellations read as a crown instead of merging into a bar. Matches the canonical
`Regent.psb`. (`shared/ui/brand/art.ts`.)

**Verified:** `tsc --noEmit` clean · `biome check` clean · `bun test` 17 pass · `bun build --compile`
ok · render smoke shows `▄ ▄ ▄` / `█▄█▄█▄` crown over a solid body.

## 2026-06-18 — feat: regent-tui Phase 2 — interactive chat (streaming, tools, approval, interrupt)

The Ink front-end becomes interactive: a `chat/` feature drives a live turn over the daemon's
JSON-RPC events, ported to behavioral parity with the Go chat (`view.go` `handleNotif`).

- **Domain (pure, tested):** `transcript.ts` — a `(state, action) → state` reducer folding daemon
  notifications (turn.started · message.delta · tool.start/complete · approval.request ·
  message.outbound · turn.interrupted · message.complete · turn.complete) and local actions
  (userMessage · approvalResolved · note). 8 unit tests cover streaming commit, tool lines, the
  approval round-trip, interrupt, and stable monotonic ids. `chatPort.ts` is the outbound port.
- **Data:** `rpcChatAdapter.ts` implements `ChatPort` over the JSON-RPC client (prompt.submit with no
  client-side timeout — turns can run minutes; turn.interrupt; approval.respond), session-scoped.
- **Presentation:** `useChat` viewmodel wires events → reducer and exposes send/interrupt/respond;
  `ChatView` renders the committed transcript via Ink `<Static>` (prints once, native scrollback) with
  a live region for in-flight streaming text + status line + input; plus `MessageInput` (controlled),
  `StatusLine` (spinner/approval/idle), and `TranscriptItem`.
- **Interaction parity:** streamed replies, tool-activity lines, inline y/N approval, Ctrl-C →
  turn.interrupt (idle → exit), `/quit`·`/exit`. Chat owns input once connected; the bootstrap
  key-handler is gated off in the ready state to avoid double-capture.
- **rpc client:** `call` now skips its timeout when `timeoutMs <= 0` (the long-running prompt.submit).
- **shared/ui reorganised** into `tokens/` (theme) · `components/` (Panel, Spinner) · `brand/` (art,
  BrandHeader), for consistency with the rest of the clean-arch tree.

Files: `src/features/chat/**` (domain/data/presentation, 8 files incl. tests); `app/presentation/App.tsx`
(hands off to ChatView when ready); `shared/ui/**` moved into subfolders; `rpc/client.ts` no-timeout path.

**Verified:** `bun test` 17 pass · `tsc --noEmit` clean · `biome check` clean · `bun build --compile`
ok · render smoke: the compiled binary boots into the chat surface (greeting + `❯ Type a message…`)
against the real daemon with no crash.

**Not yet:** full slash-command registry (only `/quit`·`/exit`; `/help`·`/new`·`/stop` + skill
commands are follow-ups) · captive alt-screen viewport + input cursor editing (Phase 4 polish) ·
interactive end-to-end (typing a real turn) needs a TTY — checked by hand, not automated.

## 2026-06-18 — feat: regent-tui Phase 1 — TypeScript/Ink front-end skeleton (coexists with Go CLI)

First slice of an Ink (React-for-terminal) front-end at `src/regent-tui/`, a thin JSON-RPC client to
`regent-daemon` that **coexists with** the Go CLI (`src/regent-cli/`) — no Rust or Go code is touched;
all three planes meet at the daemon's JSON-RPC contract. User pivot: ADR-012/next-steps had deferred
TS Ink to P8; it is now built alongside Go (see ADR-014).

- **Toolchain:** Bun + TypeScript (strict) + Ink 5 + Biome. `bun build --compile` → a single
  self-contained binary (`dist/regent-tui.exe`, ~99 MB, zero runtime deps) — matches Go's
  zero-dependency distribution, so it adds no install friction (the brief's core constraint).
- **Architecture:** feature-based clean arch applied literally — `app/` (presentation/di/config),
  `shared/` (kernel: Result + `IRpcClient` contract · ui: theme/art/Panel/Spinner/BrandHeader ·
  infrastructure: rpc/daemon/logger). Dependency rule holds; DI is the only place infra is constructed.
- **RPC:** newline-delimited JSON-RPC 2.0 over the daemon's stdio (semantics ported from the Go
  `rpc.Client`); responses route by id, notifications fan out. Daemon locate/spawn + `.env` merge
  ported from `daemon.Locate`/`appendDotEnv`.
- **UI:** the welcome screen — gradient-silver "REGENT" half-block wordmark, the kneeling-king braille
  mark, and the session panel (model/commands/skills). Brand art reproduced in TS from Regent's own Go
  identity (original code). **Crown is gold** (amber gradient) per the canonical `Regent.psb` mark —
  this corrects ADR-012's "teal crown"; teal #00A19B remains the UI accent.
- **Reference policy:** Claude Code's Ink source is studied for craft/patterns only and reimplemented
  on the published `ink` package (user-chosen "adapt onto npm ink", not vendor the fork). The
  reference's leaf patterns (ScrollBox, AlternateScreen, input) land in Phase 2.
- Hardened non-TTY stdin: Ink reports `isRawModeSupported` as `undefined` (not `false`) off-TTY, so
  the input hook is gated on a coerced boolean → no raw-mode crash on piped/CI stdin.

Files: `src/regent-tui/` (package.json, tsconfig, biome.json + 16 source/test files); `docs/adr/ADR-014`.

**Verified:** `bun test` 9 pass incl. a live `health` round-trip against the built daemon · `tsc
--noEmit` clean · `biome check` clean · `bun build --compile` produces the binary · live smoke: the
compiled binary spawns the real daemon and renders the welcome panel with the daemon's actual model
(`claude-sonnet-4-6`).

## 2026-06-18 — docs: P5 — platform set complete; iMessage documented unsupported

Closes out the messaging-platform work. **18 platforms** ship as tested `WebhookAdapter`s (Telegram,
Slack, Messenger, WhatsApp, LINE, Mattermost, Discord, Teams, Twilio SMS, Twilio Voice, Feishu,
WeChat, WeCom, Email, Jira, Azure DevOps, Trello, Google Chat) over one contract — verify
(HMAC/Ed25519/AES+SHA/RS256-JWKS/Basic) → parse → reply (Bearer/Basic × JSON/Form, or sync
JSON/TwiML), plus the `GET echostr` and `url_verification` handshakes.

**iMessage** is documented as **unsupported by design** (QUICKSTART): Apple ships no server bot/
webhook API, so there's no adapter — a self-hosted macOS bridge (e.g. BlueBubbles) is the only path,
and once present it re-exposes ordinary signed webhooks that drop into the existing contract with no
core changes. No stub shipped.

## 2026-06-18 — feat: P5 — Google Chat adapter (RS256 JWT + rotating JWKS)

Adds **Google Chat** — the first adapter that verifies a Google-signed JWT against rotating public
keys. Crypto scheme verified against Google's "Verify requests from Google Chat" doc.

- **`GoogleChatAdapter`:** the `Authorization: Bearer <jwt>` is RS256, issued by
  `chat@system.gserviceaccount.com` with `aud` = the Cloud project number. Verified with
  `jsonwebtoken` against Google's JWKS
  (`service_accounts/v1/jwk/chat@system.gserviceaccount.com`). Because `verify` is synchronous but
  the JWKS fetch is async, the keys live in a sync-readable `RwLock<HashMap<kid, DecodingKey>>` that
  a **background task refreshes** hourly (`spawn_refresher`, started at registration); an unknown/
  rotated `kid` or a cold cache denies (fail closed). Replies are returned **synchronously** as
  `{"text": …}` (the sync-reply path). Enabled by `GCHAT_AUDIENCE`.
- New deps: `jsonwebtoken` (RS256 validate); `rsa` + `rand_core` 0.6 (dev-only — mint a keypair to
  exercise the real RS256 path in tests). 3 tests: valid JWT accepted; wrong aud/iss/expiry/unknown
  kid/cold cache all rejected; MESSAGE parse + sync reply.
- This is the JWT slice deferred when Teams chose the shared-secret route — Google Chat had no honest
  shared-secret mode.

**Verified:** `cargo test --workspace` green (gateway lib: 77 tests) · clippy clean (`-D warnings`).

## 2026-06-18 — feat: P5/P6 — WeCom, Email, Jira, Azure DevOps + Trello adapters

Five more platforms, built in parallel (sub-agents for WeCom/Email/Jira/Azure DevOps; Trello added
directly) on the now-stable webhook contract — no new contract surface was needed.

- **WeCom (企业微信):** reuses `wechat_crypto`; *always* encrypted — the GET `echostr` is ciphertext
  that's decrypted and echoed, message POSTs verify `msg_signature` over `<Encrypt>` and decrypt.
  Replies via the corp `message/send` API (numeric `agentid`). Env `WECOM_TOKEN`,
  `WECOM_ENCODING_AES_KEY`, `WECOM_AGENT_ID` (+ `WECOM_ACCESS_TOKEN`).
- **Email (Mailgun):** Inbound-Parse with the signature in the **body** — HMAC-SHA256(signing_key,
  `timestamp+token`), fail-closed; `sender`/`body-plain` (subject fallback) → event; replies via the
  Messages API (Basic `api:key`, form body). Env `MAILGUN_SIGNING_KEY`/`_API_KEY`/`_DOMAIN`/`_FROM`.
- **Jira Cloud (events):** optional `X-Hub-Signature: sha256=` HMAC-SHA256 (unsigned accepted when no
  secret); issue/comment events → a summary `MessageEvent`; replies as ADF comments via REST v3
  (Basic email:token). Env `JIRA_EMAIL`/`_API_TOKEN`/`_BASE_URL` (+ `JIRA_WEBHOOK_SECRET`).
- **Azure DevOps (Service Hooks):** Basic-auth subscription check (constant-time; unconfigured
  accepted); `workitem.*`/`build.*` → summary; replies as work-item comments (PAT as Basic
  password). Env `AZURE_DEVOPS_PAT`/`_ORG_URL` (+ `_BASIC_USER`/`_BASIC_PASS`).
- **Trello:** `X-Trello-Webhook` = base64(HMAC-SHA1(secret, body ‖ callbackURL)) via `verify_request`
  (URL-aware); the HEAD/GET liveness check returns 200 via `verify_get`; `commentCard` → event;
  replies post a card comment. Env `TRELLO_API_SECRET`/`_API_KEY`/`_TOKEN`.

All five wired into `registry_from_env` + the gateway exports. 28 new tests. **gateway lib: 74
tests.**

**Verified:** `cargo test --workspace` green · clippy clean (`-D warnings`).

## 2026-06-17 — feat: P5 — WeChat Official Account adapter (WXBizMsgCrypt + GET handshake)

Adds **WeChat 公众号** support — the first platform that verifies over `GET` and signs in the query
string rather than headers. Crypto verified against the WeChat Open Platform spec.

- **Contract + route:** `WebhookAdapter` gains `verify_get(query)`; the daemon now serves
  `GET /webhook/{platform}` (`post(handle).get(handle_get)`) — the URL-verification handshake that
  signs the query and echoes `echostr` as `text/plain`. 1 daemon route test (echo / 401 / 404).
- **`wechat_crypto`:** WXBizMsgCrypt — `AESKey = base64(EncodingAESKey + "=")` (32 bytes, IV =
  `AESKey[..16]`), AES-256-CBC + PKCS7, unwrapping the `[16 random][4-byte BE len][msg][appid]`
  envelope (fail-closed); `SHA1_hex(sorted[token, timestamp, nonce, encrypt?])`; a flat-XML/CDATA
  field extractor. 3 tests.
- **`WeChatAdapter`:** GET `echostr` verification; POST verifies `signature` (plaintext) or
  `msg_signature` over `<Encrypt>` (encrypted) — both parsed from the **query** in `request.url`,
  not headers — and decrypts; parses `text` messages (`FromUserName` + `Content`); acks the POST and
  replies asynchronously via the Customer Service `message/custom/send` API (access token in the
  query). 5 tests. Enabled by `WECHAT_TOKEN` (+ optional `WECHAT_ENCODING_AES_KEY`,
  `WECHAT_ACCESS_TOKEN`).

**Verified:** `cargo test --workspace` green · clippy clean (`-D warnings`).

## 2026-06-17 — feat: P5 — Feishu / Lark adapter (encrypted callbacks + handshake)

Adds **Feishu/Lark** event-subscription support, in both plaintext and encrypted modes, with the
crypto verified against the Feishu Open Platform spec.

- **Contract:** `WebhookRequest` gains a `nonce` field; `WebhookAdapter` gains `nonce_header()` and a
  `handshake(body)` hook — a post-verify, pre-parse step for endpoint-verification challenges
  (Feishu/Slack `url_verification`, later WeChat `echostr`). The daemon route reads the nonce header,
  then answers `handshake` (via the existing sync-reply renderer) before running any turn.
- **`feishu_crypto`:** AES-256-CBC decryption (`key = SHA256(encrypt_key)`, `base64(iv ‖ ct)`,
  PKCS7, fail-closed) and the `X-Lark-Signature` = `SHA256_hex(ts ‖ nonce ‖ key ‖ body)` with a
  constant-time compare. 3 tests (encrypt/decrypt round-trip + fail-closed, signature formula,
  ct-eq). New deps `aes`, `cbc`.
- **`FeishuAdapter`:** encrypted mode verifies the signature + decrypts; plaintext mode checks the
  Verification Token in the body (top-level or schema-2.0 `header.token`); `url_verification` echoes
  the challenge; parses `im.message.receive_v1` (chat_id + the `content` JSON-string's `text`);
  replies via `im/v1/messages` with a tenant token. 4 tests. Enabled by
  `FEISHU_VERIFICATION_TOKEN` (+ optional `FEISHU_ENCRYPT_KEY`, `FEISHU_TENANT_TOKEN`).
- Outbound uses an operator-supplied `FEISHU_TENANT_TOKEN`; automatic `tenant_access_token` refresh
  (app id/secret → token endpoint, cached) is noted as follow-up.

**Verified:** `cargo test --workspace` green · clippy clean (`-D warnings`).

## 2026-06-17 — feat: sandboxed tool execution (filesystem jail + ephemeral container)

Hardens the agent's tool execution — important now that external chat platforms can trigger turns.
Defense in depth across both the in-process file tools and shell command execution.

- **Filesystem jail (in-process tools):** `ToolContext` gains an optional sandbox root;
  `resolve()` now returns `Result` and, when jailed, rejects `..` traversal, symlink escapes in the
  existing prefix, and absolute paths outside the root. `read_file`/`write_file`/`search_files`/
  `terminal` cwd all honor it (the file tools run via `std::fs`, so this — not a container — is what
  contains them). Secrets stay safe for free: `$REGENT_HOME` lives outside the workspace jail.
- **Ephemeral-container backend (shell commands):** `REGENT_TERMINAL_BACKEND=sandbox:<image>` runs
  each command in a fresh `docker run --rm --network none --read-only --cap-drop ALL
  --security-opt no-new-privileges --memory 512m --pids-limit 256` with only the workspace (`/work`)
  and a tmpfs `/tmp` writable — stronger than `docker exec` into a standing container.
- **Enforce mode (fail loud):** `REGENT_SANDBOX=1` jails the session `ToolContext` and **forbids the
  host `local` backend** — `terminal_backend_from_env` returns a hard config error (the daemon
  refuses to start unsandboxed) rather than silently degrading.
- **Secret-env stripping (all backends):** every spawned command has credential-shaped env vars
  (`*SECRET*`/`*TOKEN*`/`*PASSWORD*`/`*API_KEY*`/`*_KEY`/…) removed before exec via
  `is_secret_env_var`, so a tool command (or a prompt injection) can't exfiltrate Regent's provider
  keys or platform tokens through the shell. Replicates Hermes's "API keys stripped from the child
  env".
- **Design doc:** new [`docs/SANDBOXING.md`](SANDBOXING.md) — threat model, the five layers, the
  architecture mapping, and a capability comparison against Claude Code's `sandbox-runtime` and the
  Hermes Agent's terminal backends, plus deliberate non-goals/future work.
- **Wiring fix:** `terminal_backend_from_env` was exported but never called — every composition root
  used `core_catalog()` (hardcoded `LocalBackend`), so docker/ssh were dead code. Added
  `core_catalog_from_env()` and switched the daemon session catalogs to it, so the backend env
  actually takes effect.
- New `infra::sandbox` module (`SandboxBackend`, `sandbox_enabled`, `build_sandbox_args`,
  `enforce_backend`). 6 new tests (jail allow/deny, escape refusal, locked-down argv, enforce-mode,
  truthy parsing); existing command-approval gate + timeouts unchanged.

**Verified:** `cargo test --workspace` green · clippy clean (`-D warnings`).

## 2026-06-17 — feat: P5 — Twilio Voice (speech IVR via TwiML)

Adds inbound **voice calls** as a conversational speech IVR, reusing the Twilio signature scheme and
the sync-response path — no external STT/TTS service.

- **`SyncReply` enum** (`Json | Xml`) replaces the bare JSON sync body, so a sync-reply adapter can
  return **TwiML (XML)** with the right `Content-Type`; the route renders each accordingly. Added
  `sync_idle_response()` for when a sync adapter parses **no** user event (Voice's initial call).
  Teams updated to `SyncReply::Json`.
- **`TwilioVoiceAdapter`:** verifies via the shared Twilio check; parses `SpeechResult` (Twilio's
  built-in transcription) keyed by `CallSid` (one session per call); replies as
  `<Say>…</Say><Gather input="speech">` (XML-escaped), looping back for the next turn; greets on the
  initial call via `sync_idle_response`. 3 tests. Enabled by `TWILIO_AUTH_TOKEN` +
  `TWILIO_VOICE_GREETING`.
- **Refactor:** the Twilio HMAC-SHA1 signature check is now one shared `infra::platforms::twilio`
  helper used by both SMS and Voice (single audited verification); the SMS adapter + tests were
  moved onto it with assertions intact.

**Verified:** `cargo test --workspace` green · clippy clean (`-D warnings`).

## 2026-06-17 — feat: P5 — Microsoft Teams adapter + synchronous-reply route path

Adds **Teams** (Outgoing Webhook) and the sync-response groundwork it (and Google Chat) need.

- **Contract:** `WebhookAdapter` gains `sync_reply() -> bool` (default `false`) and
  `sync_response(reply) -> Value`. Most platforms ack `200` and deliver the reply out-of-band; the
  few that expect the reply **in the HTTP response body** opt in via `sync_reply`.
- **Route:** `/webhook/{platform}` now returns a `Response` (was a bare `StatusCode`). For a
  `sync_reply` adapter it runs the single turn **inline** and returns `sync_response(reply)` as the
  body; everything else keeps the fire-and-forget spawn. Existing adapters/tests unchanged.
- **`TeamsAdapter`:** verifies `Authorization: HMAC <base64(HMAC-SHA256(body, key))>` where `key`
  is the base64-decoded Outgoing Webhook security token (constant-time); strips `<at>…</at>` mention
  markup; replies synchronously as `{"type":"message","text":…}`. 3 adapter tests + 1 daemon route
  test for the sync path. Enabled by `TEAMS_OUTGOING_SECRET`.
- **Google Chat deferred to the JWT slice:** it has no shared-secret mode — every request is signed
  by a Google-issued JWT, so a "token" check would be security theater. It rides this same
  sync-response path once JWKS/cert validation lands.

**Verified:** `cargo test --workspace` green · clippy clean (`-D warnings`).

## 2026-06-17 — feat: P5 — Twilio SMS adapter + generalized reply transport

Adds inbound/outbound **SMS via Twilio**, and the shared transport groundwork it needed.

- **Contract (groundwork):** `WebhookAdapter` gains `verify_request(&WebhookRequest)` — a default
  that delegates to `verify(body, sig, ts)`, so every existing body-signing adapter is unchanged,
  while schemes that sign the **request URL + params** (Twilio) override it. `SendRequest` is
  generalized from `{ bearer, body: Value }` to `{ auth: SendAuth, body: SendBody }` —
  `SendAuth::{None,Bearer,Basic}` and `SendBody::{Json,Form}` — so Basic-auth + form-urlencoded
  replies are expressible (Twilio now; WeChat/WeCom/Azure DevOps later). The five existing adapters
  (Slack/Messenger/LINE/WhatsApp/Mattermost) were migrated to the new shape with their tests intact
  (same assertions, new field names). `reqwest` gains the `form` feature.
- **`TwilioSmsAdapter`:** verifies `X-Twilio-Signature` = base64(HMAC-SHA1(authToken, url +
  sorted(params))) via `verify_request` (constant-time; the body-only `verify` denies by design);
  parses `From`/`Body` form fields into a `MessageEvent`; replies via the Messages REST API with
  HTTP Basic auth and a form body. 3 tests (signature accept/tamper, parse + status-callback skip,
  send-request shape). Enabled by `TWILIO_ACCOUNT_SID`/`TWILIO_AUTH_TOKEN`/`TWILIO_FROM_NUMBER`.
- **Daemon:** `/webhook/{platform}` now reconstructs the full public URL (from `x-forwarded-proto`/
  `-host`/`host`) and calls `verify_request`; `deliver` handles the JSON/Form × None/Bearer/Basic
  matrix. New deps: `sha1`, `form_urlencoded`.

**Verified:** `cargo test --workspace` green · clippy clean (`-D warnings`).

## 2026-06-17 — chore: migrate schemars 0.8 → 1.x (cross-repo, with Orchustr)

Orchustr bumped its workspace `schemars` to **1.2.1** while `or-mcp`'s source still used the 0.8
`schema` API (`RootSchema`/`SchemaObject`/`InstanceType`/`SingleOrVec`, all removed in 1.0), which
broke the Regent build (`or-mcp` no longer compiled). Migrated both repos to the 1.x API instead of
holding schemars back.

- **Orchustr `or-mcp`:** `McpTool.input_schema` is now `schemars::Schema` (1.x wraps a JSON value).
  `server_validation.rs` rewritten to introspect the schema's JSON keywords directly (`type`,
  `required`) via `Schema::{as_bool, get, as_object}` — same enforcement surface as before. The two
  unit tests build their schema with `schemars::json_schema!({ "type": "object" })`.
- **Regent:** workspace pin `schemars = "0.8.22"` → **`"1"`** (kept in lockstep with Orchustr's
  pin); `regent-tools` integration test uses `schemars::Schema::default()` (empty/accept-all `{}`,
  same as the old `RootSchema::default()`). `mcp_tools.rs`/`mcp_server.rs` were unaffected — they
  round-trip `input_schema` through serde, and `Schema` is transparently `Serialize`/`Deserialize`.
- **Lock:** `schemars` now resolves to a single **1.2.1**; the 0.8.22 node is gone.

**Verified:** Regent `cargo test --workspace` green · clippy clean (`-D warnings`) · Orchustr
`cargo test -p or-mcp` green.

## 2026-06-17 — feat: P5 — Discord interactions webhook (Ed25519, slash commands)

Adds the HTTP "interactions" mode for Discord (slash commands), distinct from the Gateway chat
adapter. Discord signs each interaction with **Ed25519** over `timestamp || body` and requires a
synchronous response.

- **`regent-daemon` `infra/discord_interactions.rs`:** verifies the signature against
  `DISCORD_PUBLIC_KEY` (fails closed on any malformed input), answers `PING` (type 1) with `PONG`,
  and for a command (type 2) acks with a **deferred** response (type 5), runs the turn in the
  background keyed `discord:{channel}`, then delivers the reply as a follow-up to
  `webhooks/{app_id}/{token}`. 4 tests (valid/ tampered signature, ping + command parse, route 200 /
  401). Added `ed25519-dalek = "2"`.
- **Wiring:** `spawn_http_listener` merges `/discord/interactions` only when `DISCORD_PUBLIC_KEY` is
  set (deny-by-default — the route doesn't exist otherwise).

**Verified:** `cargo test -p regent-daemon` green (12 suites incl. 4 new) · clippy clean.

## 2026-06-17 — feat: P5 — per-conversation session continuity for platforms

Webhook (and gateway) chats now keep **one continuous session per conversation** instead of a fresh
session each message — so a Slack thread / Discord channel / WhatsApp chat remembers context.

- **Store** (schema v7→v8): `conversation_sessions(conversation_key PK, session_id, created_at)` +
  `bind_conversation` / `conversation_session`. 1 test (bind, lookup, rebind, key isolation).
- **SessionManager** `ensure_keyed_session(key)`: reuse the live session if active → resume the bound
  one if cold → otherwise create a fresh session and bind it (a purged/stale binding falls through to
  recreate).
- **`ChatService::chat_keyed(key, msg)`**: default starts fresh (so REST `/v1/chat` and test stubs
  are unchanged); the session-manager-backed impl routes through `ensure_keyed_session`.
- **Webhook route** now calls `chat_keyed("{platform}:{chat_id}", text)` — the v1 "fresh session per
  message" limitation is gone.

**Verified:** `cargo test --workspace` green (44 suites) · clippy clean (`-D warnings`).

## 2026-06-17 — chore: dependency update (latest stable)

Verified every workspace dep against crates.io and moved each to its latest stable.

- **`cargo update`** floated all caret-pinned deps to the latest within their major (tokio 1.52,
  axum 0.8.9, uuid 1.23, regex 1.12, tempfile 3.27, serde_json 1.0.150, async-trait 0.1.89, …).
- **Major bumps** (out of caret range) applied + migrated: `rusqlite` 0.33 → **0.40** (no store API
  changes), `tokio-tungstenite` 0.24 → **0.29** (the Discord `Message` handling already fit),
  `hmac` 0.12 → **0.13** + `sha2` 0.10 → **0.11** (digest 0.11 — `new_from_slice` moved to the
  `KeyInit` trait; added `use hmac::digest::KeyInit` to the four HMAC adapters), `reqwest` floor →
  **0.13.4**.
- **Held back, with reasons documented in `Cargo.toml`:** `schemars` stays **0.8** — `or-mcp`
  (Orchustr) types `McpTool.input_schema` as a schemars-0.8 `RootSchema` (removed in 1.0), and
  `mcp_integration.rs` constructs it; bump only when Orchustr's or-mcp moves to 1.x. `serde_yaml`
  0.9 is its last (archived) release.
- **Go CLI:** `go get -u ./...` + `go mod tidy` — 10 transitive bumps (golang.org/x/sys 0.46,
  x/text 0.38, charmbracelet/*, etc.).

**Verified:** `cargo test --workspace` green (44 suites) · clippy clean (`-D warnings`) · `go
build`/`vet`/`test` green.

## 2026-06-17 — feat: P5 — Discord Gateway (WebSocket) adapter

Discord chat via the Gateway (real `MESSAGE_CREATE` messages, not slash-command interactions —
that's a later slice). `DiscordGateway` (`regent-gateway/infra/platforms/discord.rs`) implements the
polling `PlatformAdapter`: a background task holds the WebSocket (HELLO → IDENTIFY → heartbeat loop,
reconnect on drop) and pushes each user message onto a channel that `next_event` drains; replies post
to `/channels/{id}/messages` with `Bot` auth. Skips bot authors and empty content. Adds
`tokio-tungstenite` (rustls) + `futures-util`.

- Pure protocol logic is unit-tested: `identify_payload` (carries the privileged `MESSAGE_CONTENT`
  intent), `heartbeat_payload` (null → last sequence), `parse_message_create` (user message →
  event; skips bots / non-message dispatches / empty content). 3 tests.
- The live WebSocket loop compiles and follows the v10 gateway protocol; **end-to-end needs a real
  bot token to validate** (not run here). No resume in v1 — a disconnect re-identifies.

**Verified:** `cargo test -p regent-gateway` green (25).

## 2026-06-17 — feat: P5 — webhook ingress wired into the daemon (`/webhook/{platform}`)

The webhook platform adapters are now **live**: one generic `POST /webhook/{platform}` route on the
daemon HTTP listener serves them all (`regent-daemon/infra/webhook.rs`).

- **Contract:** `WebhookAdapter` gained `signature_header()` / `timestamp_header()` so the route
  extracts the right headers per platform (Messenger/WhatsApp `x-hub-signature-256`, LINE
  `x-line-signature`, Slack `x-slack-signature` + `x-slack-request-timestamp`, Mattermost: token in
  body → `None`).
- **Route:** look up the adapter → `verify` (401 on failure) → `parse_webhook` (400 on bad body) →
  **ack 200 immediately**, then run the turn + deliver the reply off the request path (the shape push
  platforms expect). Unknown platform → 404.
- **Registry from env:** adapters are built only when their secrets are present
  (`SLACK_SIGNING_SECRET`+`SLACK_BOT_TOKEN`, `MESSENGER_*`, `LINE_*`, `WHATSAPP_*`, `MATTERMOST_*`),
  loaded from `$REGENT_HOME/.env`. Merged into the listener when non-empty.
- **Sender:** a thin reqwest `deliver` posts the adapter's `SendRequest` (bearer + JSON).
- 3 route tests (valid signature → 200, bad/missing → 401, unknown platform → 404) with a stub
  adapter + stub `ChatService` — no network.

> **v1 limitation:** each inbound message runs in a **fresh** session (no cross-message memory yet) —
> per-conversation continuity needs a platform-key→session map (tracked follow-up).

**Verified:** `cargo test --workspace` green (44 suites) · clippy clean (`-D warnings`).

## 2026-06-17 — feat+docs: Mattermost adapter, `infra/platforms/` reorg, QUICKSTART

- **Mattermost adapter** (`regent-gateway/infra/platforms/mattermost.rs`): outgoing-webhook — the
  shared `token` rides in the JSON body and is constant-time compared to the configured verify
  token; parses `channel_id`/`text`; replies post to `/api/v4/posts` with a bot token. 3 tests.
- **Reorg:** all platform adapters moved under `regent-gateway/src/infra/platforms/` (line,
  messenger, slack, telegram, whatsapp, mattermost) with a `platforms/mod.rs`; `infra/mod.rs` is now
  just `pub mod platforms;`. Crate re-exports updated; adapter code unchanged (they use `crate::`
  paths). Chat platforms implemented: **Telegram · Messenger · LINE · WhatsApp · Slack · Mattermost.**
- **`docs/QUICKSTART.md`** — build → `setup` → `doctor` → `chat`, the secrets model, providers, `mcp
  serve`, logs, and a **platform support matrix**: the 6 implemented adapters plus the exact
  requirement/blocker for every other requested platform (Discord = Ed25519/Gateway; Teams/Google
  Chat = JWT/OAuth or sync-response; Feishu/WeCom/WeChat = bespoke SHA1/SHA256 + nonce + AES + XML;
  SMS/Voice = Twilio HMAC-SHA1 over URL + TwiML/STT; Email = async provider parse; **iMessage = no
  official API, needs a self-hosted bridge**). None ship as stubs — each lands as its own tested
  slice once its dependency/contract is added.

**Verified:** `cargo test -p regent-gateway` green (22) · clippy clean (`-D warnings`).

## 2026-06-17 — test+feat: per-feature Go `tests/` folders + Slack adapter

**Black-box `tests/` folder per Go feature** (cron, sessions, memory, inspect, mcp, logs, setup):
each drives its exported `Command()` and asserts the wiring (command name, subcommands, flags) — real
regression cover for the CLI surface, no daemon needed. These complement the inline white-box unit
tests (which must stay beside their code: a separate `tests/` package only sees a package's *exported*
API, so it can't reach unexported helpers like `secureWriteFile`/`appendDotEnv`). The TUI /
composition / network packages (app, chat, doctor) have no black-box surface and get none.

**Slack adapter** (`regent-gateway/infra/slack.rs`): Events API webhook. Slack signs
`v0:{timestamp}:{body}` (HMAC-SHA256, hex) and enforces a replay window, so the `WebhookAdapter::verify`
contract gained a `timestamp: Option<&str>` param (Messenger/LINE/WhatsApp ignore it). `verify` checks
the signature **and** rejects timestamps outside ±5 min; `parse_webhook` reads `event_callback`
messages (skips bot messages, edits, and `url_verification` challenges); replies post to
`chat.postMessage`. 3 tests incl. stale-timestamp rejection.

**Chat platforms now: Telegram · Messenger · LINE · WhatsApp · Slack.**

**Verified:** `go vet`/`go test ./...` green (incl. 7 new `tests/` packages) · `cargo test --workspace`
green (44 suites, gateway 19) · clippy clean (`-D warnings`).

## 2026-06-17 — test+feat: Go CLI unit tests + WhatsApp adapter

**Go CLI test coverage** across every pure helper: `daemon.Home` (profile→path, env-override,
named-profile isolation), `rpc.appendDotEnv` (merge missing keys only, real env wins, skip
comments/blanks, strip quotes), `ui` (`visibleLen`/`padTo` ignore ANSI, `Label`, `Panel` framing),
`logs.latestLog` (newest by name, errors when empty), `chat.short` (truncate >18). The cobra +
daemon-client features (cron/sessions/memory/inspect/mcp/doctor) and the bubbletea TUI are
integration glue — exercised by the RPC round-trip tests and the mcp e2e smoke, not unit tests.

**WhatsApp adapter** (`regent-gateway/infra/whatsapp.rs`): Meta Cloud API webhook — same
`X-Hub-Signature-256` HMAC-SHA256 verification as Messenger, parses `entry[].changes[].value.
messages[]` text (skips status callbacks), builds the Cloud API messages request (bearer token,
phone-number-id in the path). 3 tests.

Chat platforms now: Telegram (poll) · Messenger · LINE · WhatsApp. Slack is the next candidate but
needs a contract tweak — its signature covers `timestamp:body` with a replay window, so `verify`
needs the timestamp header too.

**Verified:** `go vet`/`go test ./...` green · `cargo test -p regent-gateway` green (16) · clippy
clean (`-D warnings`).

## 2026-06-17 — security: P7 — TOCTOU-safe `0600` secret writes (`.env`)

Hardened how `regent setup` persists the API key, matching Hermes's `auth.json` write discipline.
`secureWriteFile` (`src/regent-cli/features/setup`) writes `$REGENT_HOME/.env` to a temp file created
with `O_EXCL` at `0600` (born owner-only, not via the umask), `fsync`s it, then **atomically renames**
over the target — closing the window a plain write-then-`chmod` leaves where the key is briefly
world-readable. `$REGENT_HOME` is tightened to `0700`. On Windows POSIX modes are advisory (the
user-profile ACLs already restrict access). The existing upsert (preserve other `.env` lines, replace
the key) is unchanged. 2 tests: content + atomic overwrite + no temp leftover + `0600` on POSIX, and
the upsert.

> This is hardening step #1 of the Hermes-parity secrets model (`.env`/config split + redacted logs
> are already in place). Step #2 — a `regent auth` credential pool that can also read the OS keychain
> / other tools' stores — remains a future slice (P7).

**Verified:** `go build`/`go vet` clean · `go test ./features/setup/...` green.

## 2026-06-17 — feat: P5 — chat-platform webhook adapters (Messenger, LINE)

Broadens platform support beyond Telegram (which already runs via long-poll) with a webhook adapter
family for push platforms.

- **`WebhookAdapter` contract** (`regent-gateway/domain/contracts.rs`): `verify(body, signature)` →
  `parse_webhook(body)` → `send_request(msg)`, plus a platform-agnostic `SendRequest {url, bearer,
  body}`. Parse/verify/build are **pure** — fully unit-testable without a token; only the network
  send needs live credentials.
- **Messenger** (`infra/messenger.rs`): `X-Hub-Signature-256` HMAC-SHA256 (hex) verification
  (constant-time), parses `entry[].messaging[]` text events, builds the Graph Send API request
  (bearer page token).
- **LINE** (`infra/line.rs`): `X-Line-Signature` base64-HMAC-SHA256 verification, parses
  `events[]` text messages routing on group→room→user id, builds the push API request.
- Signature checks use vetted crypto (`hmac`/`sha2`, base64/hex), never hand-rolled; missing/invalid
  signatures are denied (deny-by-default). 6 new tests (verify valid/invalid/missing, parse, build)
  per-platform.

Adding WhatsApp/Slack/etc. is now just another `WebhookAdapter`. Remaining wiring (follow-up): a
daemon HTTP-listener `/webhook/:platform` route (verify → parse → run turn → send reply) + per-
platform token config + a thin `SendRequest` sender.

**Verified:** `cargo test --workspace` green (44 suites) · clippy clean (`-D warnings`).

## 2026-06-17 — feat: P8 — `regent mcp serve` exposes the full catalog (memory + skills)

The MCP server now exposes Regent's **full** capability set, not just the core tools. The
`regent-mcp` bin builds the catalog from `$REGENT_HOME` — `core_catalog()` plus `register_memory_tools`
(store + graph) and `register_skill_tools` — so an MCP client sees memory and skills too. Session-
coupled tools (delegate, send_message, kanban) are deliberately omitted; they belong to a running
agent. Still `DenyAll` approval.

**Verified:** builds · `cargo test --workspace` green (44 suites) · clippy clean (`-D warnings`) ·
end-to-end smoke: `tools/list` returns 10 tools — `terminal`, `read_file`, `write_file`,
`search_files`, `memory`, `memory_search`, `session_search`, `skill_manage`, `skill_view`,
`skills_list`.

## 2026-06-17 — feat: P7 — `regent setup` wizard + `.env` loading

First-time setup, and the secrets path it depends on.

- **`regent setup`** (`src/regent-cli/features/setup`): picks a provider (validated against the
  known set) + default model, collects the API key (flag `--key`, else `REGENT_API_KEY`, else
  prompted), then writes the key to `$REGENT_HOME/.env` (0600, **upserted** so other lines survive)
  and a minimal `config.yaml` (only when absent — never clobbers an existing config). Non-interactive
  via `--provider/--model/--base-url/--key`.
- **`.env` loading** (`shared/rpc` `Spawn`): the CLI now merges `$REGENT_HOME/.env` into the daemon's
  environment when spawning it — skipping keys already exported (a real env var always wins). This is
  what makes the key `setup` writes actually reach the daemon (`REGENT_API_KEY`).

**Verified:** `go build`/`go vet` clean · smoke: `regent setup --provider groq --model … --key …`
writes a valid `config.yaml` + `.env`.

## 2026-06-17 — chore: move source under `src/`

Reorganized the tree so all source lives under `src/`: `crates/` → `src/crates/` (the 11 Rust
crates) and `regent-cli/` → `src/regent-cli/` (the Go CLI). Updated the workspace `members` paths in
the root `Cargo.toml` and the Go job paths in `.github/workflows/ci.yml`. Inter-crate `path` deps
(`../regent-*`) and the Orchustr path-dep (anchored at the unchanged root manifest) are unaffected;
`target/` stays at the workspace root. Build configs only — no code changes.

> Design docs under `docs/` still cite the old `crates/…` paths in places; they're historical/design
> records and weren't rewritten.

**Verified from the new layout:** `cargo test --workspace` green (44 suites) · clippy clean
(`-D warnings`) · `go build`/`go vet` clean in `src/regent-cli`.

## 2026-06-17 — feat: P7 — structured rolling logs (redacted) + `regent logs`

The daemon now writes structured logs to **both** stderr (the JSON-RPC stream owns stdout) and a
daily-rolling file under `$REGENT_HOME/logs/`, with the file writer wrapped so **secrets are
redacted before they hit disk**.

- **`RedactingWriter<W>`** (`regent-kernel/redact.rs`): a `std::io::Write` wrapper that runs
  `redact_secrets` on each write before delegating — a leaked key never lands on disk. +1 test.
- **Daemon logging** (`regent-daemon/infra/logging.rs`): a layered subscriber — stderr (ANSI) +
  a redacting `tracing-appender` daily file (`regent.log.<date>`), each with its own env filter.
  Returns the appender guard; the bin holds it for the process lifetime. Adds `tracing-appender`.
- **`regent logs [--follow]`** (Go): prints the newest rolling log file, `-f` streams appended
  lines.

**Verified:** `cargo test --workspace` green (44 suites) · clippy clean (`-D warnings`) · `go
build`/`go vet` clean.

## 2026-06-16 — feat: P8 — `regent mcp serve` (Regent as an MCP tool provider)

Regent can now expose its own tool catalog as an MCP server (or-mcp `NexusServer`), so it's a tool
*provider*, not only a consumer.

- **Core** (`regent-tools/infra/mcp_server.rs`): a server-side `StdioServerTransport` (reads this
  process's stdin / writes stdout — or-mcp's `StdioTransport` is client-only), `to_mcp_tool`
  (Regent `ToolDefinition` → `McpTool`, schema deserialized straight into the MCP schema type),
  and `build_server`/`serve_catalog` that register every catalog tool with a handler dispatching
  through `ToolCatalog` — **so the dangerous-command guard + approval path still apply**. 2 tests
  exercise the real JSON-RPC `tools/list` and `tools/call` via `handle_message` (no socket).
- **Entry point** (`regent-daemon` bin `regent-mcp`): serves the core catalog over stdio with
  `DenyAll` approval (a remote caller's dangerous shell command is blocked at the guard, not run).
  stdout is the MCP stream; logs go to stderr.
- **CLI** (`regent mcp serve`, Go): execs `regent-mcp` with inherited stdio so an MCP client can
  spawn it directly; `daemon.LocateBinary` generalizes the daemon locator (env override → sibling →
  PATH → cargo dev build). Passes the active profile's `REGENT_HOME`.

Exposing the *full* catalog (memory/skills) needs the composition root and is a follow-up.

**Verified:** `cargo test --workspace` green (44 suites) · clippy clean (`-D warnings`) · `go build`/
`go vet` clean · **end-to-end smoke:** piping a `tools/list` request to `regent-mcp` returns the live
catalog as MCP JSON-RPC.

## 2026-06-16 — feat: P7 — file-state checkpoints (snapshot / rollback)

`CheckpointStore` (`regent-tools/infra/checkpoint.rs`): snapshot a set of files before a risky edit,
then roll back to restore them — a botched edit (or a whole turn) is recoverable.

- `snapshot(label, paths)` copies each existing file's bytes under the store root and records which
  paths were *absent*; returns a checkpoint id.
- `rollback(id)` rewrites the saved bytes, and **deletes** any path that didn't exist at snapshot
  time (so a file the edit *created* is undone too).
- `list()` returns checkpoints newest-first. Filesystem-backed (`$REGENT_HOME/checkpoints/<id>/` +
  `manifest.json`), dependency-light (std::fs + serde + uuid). 3 tests: restore-modified,
  delete-created, list + unknown-id error.

**Verified:** `cargo test --workspace` green (43 suites) · clippy clean (`-D warnings`).

## 2026-06-16 — feat: P5 — daemon HTTP listener (REST ingress)

The daemon gains an **opt-in HTTP listener** (the P5 foundation, deferred from M5 per ADR-009) so
platform webhooks/REST clients can drive a turn without the stdio JSON-RPC transport.

- **Routes** (`regent-daemon/infra/http_listener.rs`): `GET /health` (open, for load balancers) and
  `POST /v1/chat` (`{session?, message}` → runs the turn, returns `{session, reply}` synchronously —
  `run_turn` yields the reply directly, so no out_tx correlation needed). The HTTP layer depends only
  on an injected `ChatService` trait, so the router is unit-tested with a stub (no socket): health
  open, bearer required + constant-time compared, turn round-trip, empty-message rejected.
- **Security (deny-by-default):** `/v1/chat` requires `Authorization: Bearer <token>`; the listener
  binds to **loopback** by default and **refuses to start without a token**
  (`regent-daemon/application/http_serve.rs`). Bind `0.0.0.0` deliberately to face a network.
- **Config:** new `[http]` block — `enabled` (false), `bind` (`127.0.0.1:7878`), `token` (required
  when enabled). Wired into the bin behind the flag.
- **Deps:** adds `axum` 0.8 (minimal features) + tokio `net`; `tower` as a dev-dep for router tests.

Platform-specific adapters (Discord/Slack/WhatsApp/Signal) and voice transcription plug in on top of
this ingress but need real bot tokens / a transcription provider — tracked separately.

**Verified:** `cargo test --workspace` green (43 suites) · clippy clean (`-D warnings`).

## 2026-06-16 — feat: P7 — secrets redaction at the logging boundary + CI pipeline

**Secrets redaction** (`regent-kernel/redact.rs`, security): `redact_secrets()` masks credential
*shapes* in any string before it's logged — the threat being a provider/HTTP **error body that
echoes our `x-api-key`/`Authorization`**. Masks known prefixes (Anthropic `sk-ant-…`, OpenAI
`sk-…`, OpenRouter `sk-or-…`, Slack `xoxb-/xoxp-/xapp-`, GitHub `ghp_/gho_/github_pat_`, JWT
`eyJ…`) keeping the recognizable prefix, plus the token right after `Bearer`. Deliberately
low-false-positive (only unambiguous shapes) and dependency-free. Wired into all three provider
error-body sites (`openai_compat`, `anthropic_chat` non-stream + stream). 6 tests incl.
ordinary-text-untouched and bare-prefix-not-masked.

**CI pipeline** (`.github/workflows/ci.yml` + `deny.toml`) — none existed; the roadmap wants it
immediately. Three jobs: **rust** (fmt-check · clippy · test, toolchain pinned via
rust-toolchain.toml), **supply-chain** (`cargo audit` + `cargo deny check` — advisories/licenses/
sources), **go** (build · vet · govulncheck). `deny.toml` allows only permissive licenses and
denies unknown registries/git sources.

> **CI caveat (needs your input):** Regent depends on Orchustr via a relative path
> (`../Orchustr/orchustr/…`), so the Rust jobs check out Orchustr as a sibling. Set the repo
> variable `ORCHUSTR_REPO` (and optionally `ORCHUSTR_REF`); until then the Rust jobs are skipped
> (Go still runs). For a private Orchustr, add a deploy key/token to its checkout step.

**Verified:** `cargo test --workspace` green (43 suites) · clippy clean · local code fmt-clean.
(CI workflow + cargo-deny/audit run on GitHub, not locally.)

## 2026-06-16 — feat: adaptive-thinking passthrough + named OpenAI-compatible providers

**Extended-thinking passthrough.** The kernel `ChatMessage` gains a `thinking_signature` slot (paired
with `reasoning`); the Anthropic adapter captures the thinking block's signature (non-streaming and
streaming) and **replays it verbatim** as the first block of the assistant turn — required for valid
multi-turn tool use with extended thinking. Enabled via `ChatRequest::with_thinking(budget)` /
`AgentConfig.thinking_budget` (off by default); when on, the request omits a custom temperature
(Anthropic forbids it). Unsigned reasoning is never replayed (it would fail validation). Not
persisted — only the in-turn most-recent thinking block needs replay. Tests: signature captured
(both paths), signed block replayed first, unsigned dropped, thinking param + temperature handling.

**Named providers.** `OpenAiCompatChatConfig` gains presets — `openai`, `openrouter`, `groq`,
`deepseek`, `together`, `ollama` (the adapter already served any OpenAI-compatible endpoint; these
make the common ones first-class). The daemon's `ProviderKind` adds the matching variants so
`provider: groq` (etc.) is selectable in config.yaml; an explicit `base_url` still overrides. Any
other OpenAI-compatible host works via `provider: openai` + `base_url`.

**Refactors (200-line MUST):** `implementations.rs` (331) → `openai_compat.rs` (170) +
`anthropic_chat.rs` (178) + shared `http.rs` (retry loop + truncate — also DRYs the duplicated retry
code). `request.rs` → `request.rs` + `messages.rs` (transcript translation). `stream.rs` tests moved
to `stream/tests.rs`. Daemon provider factory extracted from the bin into `provider_factory.rs`
(bin 198 → 172). All ≤200.

**Verified:** `cargo test --workspace` green (43 suites) · clippy clean.

## 2026-06-16 — feat: P6 orchestrator depth-2 + child-cancel propagation

Delegation can now nest one level deeper, and interrupting a parent aborts its running tools and
sub-agents.

- **Bounded depth-2** (`regent-agent/.../delegation/`): new `max_depth` (default 2). A child below
  the cap receives the leaf catalog **plus** its own `depth+1` `delegate_task` (so it can fan out
  once more); a child at the cap gets the leaf catalog only — the hard recursion stop. Enabled by
  making `ToolCatalog: Clone` (cheap — executors/hooks are `Arc`) so a child catalog = leaf + a
  deeper delegate tool. `DelegateTool::new` signature unchanged (call sites untouched).
- **Child-cancel propagation** (`regent-agent/.../agent/turn.rs`): the tool-dispatch `join_all` now
  runs inside the cancel `select!`. A cancel drops the in-flight dispatch future, which drops every
  tool — including delegated children (they're futures within that tree) — so cancellation
  propagates downward. Benefits all tools (e.g. a long terminal command), not just delegation.
- **Tests:** depth-cap unit tests (below-cap nests, at-cap stops, `max_depth=1` = leaf-only),
  depth-2 end-to-end (a child successfully delegates), and child-cancel (a slow tool is dropped
  mid-run, turn returns `Interrupted`).
- **Refactors (200-line MUST):** `delegation.rs` → `delegation/{mod,tool}.rs`; `agent.rs` (282) →
  `agent/{mod,turn}.rs` (struct/constructors vs. the turn loop). Behavior-preserving; all ≤200.

**Verified:** `cargo test --workspace` green · clippy clean.

> P5 platform breadth (Discord/Slack/Signal adapters, HTTP listener, cron→platform delivery, voice)
> and adaptive-thinking passthrough remain — they need a platform/credentials decision and a kernel
> thinking-signature slot respectively, tracked as their own slices.

## 2026-06-16 — feat: P6.4 board dispatcher wired into the daemon + AgentReviewer

The board dispatcher now runs as a daemon background loop (mirroring the cron loop), and the
`agent` review policy has a real implementation.

- **`AgentReviewer`** (`regent-agent/.../board/reviewer.rs`): runs the worker's result through a
  fresh agent (review source) with a strict verdict prompt, then maps the reply via a deterministic
  `parse_verdict` — first line starting `APPROVE`/`REJECT` wins; **anything ambiguous is a
  rejection** (never auto-approve on an unclear review). 5 parser tests.
- **`dispatch_pending(board, max)`** on `BoardDispatcher`: drains up to `max` claimable tasks per
  tick so one busy board can't starve the runtime. Integration test (cap honored, stops when dry).
- **Daemon loop** (`regent-daemon/.../board_dispatch.rs` + bin): opt-in via new `[board]` config
  (`enabled` **false by default**, `tick_interval_secs` 15, `max_per_tick` 4). Worker + reviewer run
  with `DenyAll` (no autonomous terminal/destructive actions). Autonomous execution and its token
  spend are never enabled silently.
- **Refactors (200-line MUST):** extracted the boot background loops (embedder attach, TTL purge,
  pending-write expiry) into `application/background.rs`, bringing the composition root back under
  200 (bin 198). Split `domain/entities.rs` (config schema + JSON-RPC types, two concerns) into
  `domain/config.rs` (144) + `entities.rs` (77, RPC only). Behavior-preserving; all import sites
  updated.

**Verified:** `cargo test --workspace` green (42 suites) · clippy clean.

## 2026-06-16 — feat: per-board review policy (human / agent / auto)

Each board now declares **how finished work reaches `done`** — a person approves (`human`,
default), a reviewer agent judges it (`agent`), or it self-approves (`auto`). The dispatcher reads
the policy after a clean run; unconfigured boards default to `human`, so existing tasks are
unaffected.

- **Schema** (`schema.rs`, v6→v7): new `boards(board PK, review_policy DEFAULT 'human',
  reviewer_agent, created_at)`. Additive.
- **Store** (`regent-store/infra/boards.rs`, new): `ensure_board`, `set_board_policy`, `find_board`,
  `board_policy` (defaults to `human` when unconfigured — the fail-safe). `ReviewPolicy { Human,
  Agent, Auto }` + `BoardRow` domain types (`parse` defaults unknown strings to `Human`). 4 tests.
- **Dispatcher** (`regent-agent/.../board/`): added a `Reviewer` trait + `ReviewVerdict`
  (Approve/Reject), injected via `BoardDispatcher::with_reviewer`. Clean run → land in `in_review`,
  then resolve by policy: `human` waits · `auto` → `done` · `agent` runs the reviewer (approve →
  `done`, reject → back to `in_progress` for rework, *not* auto-re-dispatched so a bad reviewer
  can't cause a retry storm). `agent` policy with no reviewer wired falls back to `human` (never
  auto-completes).
- **Refactor:** `board.rs` exceeded the 200-line MUST, so it's now a `board/` folder — `mod.rs`
  (contracts), `dispatcher.rs`, `runner.rs` (all ≤113 lines); the 7 dispatcher tests moved to
  `tests/board_dispatch.rs` (public-API integration). Behavior-preserving.

**Verified:** `cargo test --workspace` green (42 suites) · clippy clean.

## 2026-06-16 — feat: P6.3 board dispatcher + a review column (review-before-done)

**Kanban gains a review stage.** The board flow is now `todo → in_progress → in_review → done`,
with `blocked` reachable from any column. Work is **verified before it's marked done** — a worker
finishes and *submits*; a reviewer (human via the tool, or a future reviewer agent) *approves* →
done, or *rejects* → back to `in_progress`. This mirrors the memory write-approval gate: nothing
self-completes.

- **Store** (`regent-store/infra/kanban.rs`): added `transition_task(id, from, to)` — an atomic
  *guarded* move that only fires when the task is in the expected column (so you can't approve
  something that was never submitted). `set_task_status` stays for `block` (valid anywhere). +1 test.
- **`kanban` tool** (`regent-tools/infra/kanban_tools.rs`): the single `complete` action is replaced
  by the guarded review flow — `submit` (in_progress→in_review), `approve` (in_review→done),
  `reject` (in_review→in_progress). create / list / claim / block unchanged. 2 review-flow tests
  (incl. "approve from in_progress is refused").
- **Board dispatcher** (`regent-agent/application/board.rs`, P6.3): on a clean run the dispatcher
  now parks the task in `in_review` (it never auto-completes); failure still auto-blocks. Outcome
  status is `in_review | blocked`. Tests updated.

**Verified:** `cargo test --workspace` green · clippy clean.

## 2026-06-16 — feat: P5.2 daemon delivery + P6.2 kanban tool, both wired into sessions

**P6.2 — `kanban` worker tool** (`regent-tools/infra/kanban_tools.rs`): create / list (status
filter) / claim / complete / block over the shared board; claim is the store's atomic single-winner
UPDATE. 3 tests incl. single-winner-through-the-tool.

**P5.2 — daemon-native delivery:** `NotificationDelivery` sink (the connected surface *is* the
channel → a `send_message` becomes a `message.outbound` notification the CLI renders). Both
`send_message` and `kanban` are now registered in every session's catalog
(`session_manager/build.rs`); the bubbletea CLI renders `message.outbound` (`✉ delivered to …`).
Daemon delivery-sink unit test added.

**Fix — non-blocking embedder boot:** the daemon previously blocked startup on the ONNX model load
(P4.5), so `health` timed out on a fresh home / first run. `GraphMemory.embedder` is now a
late-bindable `OnceLock` with `attach_embedder(&self)`; the daemon serves immediately and the model
attaches from a background task (memory runs on FTS + graph until it binds). Verified: `regent
doctor` green on a **fresh** home (health round-trip OK).

**Verified:** `cargo test --workspace` → 155 passed · clippy clean · `go build/vet` clean ·
`regent doctor` green on a fresh home.

## 2026-06-16 — feat: P5.1 send_message delivery + P6.1 kanban board (first slices)

First foundational slices of two large phases — each self-contained and tested. (P5/P6 full breadth
— platform adapters, HTTP listener, orchestrator depth-2 — remains.)

**P6.1 — kanban board** (`regent-store/infra/kanban.rs`, schema v5→6): `kanban_tasks` table +
board-scoped CRUD. The load-bearing invariant is an **atomic claim** — a single conditional UPDATE
(`WHERE status = 'todo'`) so two workers never grab the same task. `create_task`, `list_tasks`
(board + optional status filter), `claim_task`, `set_task_status`, `find_task`. 3 tests incl.
single-winner race.

**P5.1 — `send_message` tool** (`regent-tools`): a `DeliverySink` contract (alongside
`ApprovalHandler`) the surface implements; a `send_message` tool that names a target and delivers
through the sink — the model sees the available targets in the schema, never a platform SDK.
`NoDelivery` fail-safe declines when nothing is configured. 4 tests (deliver, empty-text guard,
no-sink decline, schema lists targets).

**Verified:** `cargo test --workspace` green · clippy clean.

## 2026-06-16 — feat: retrieval eval harness (ml-pipeline principles, native Rust)

**Goal:** Formalize the retrieval regression evals into one reusable harness — the
`/ml-pipeline` work. Applied the transferable MLOps principles (versioned in-repo dataset, schema
validation before scoring, explicit pass/fail thresholds, per-class metrics, reproducibility via
logged params) **natively in Rust**; the Python MLOps stack (MLflow/Kubeflow/Feast) is out of scope
for a local agent (YAGNI).

**What was done:**
- **`regent-graph/application/evals.rs`** (new `pub mod evals`, 4 unit tests): pure metrics
  (`recall_at_k`, `mrr`); `GoldenCase` with an `EvalClass` label (Exact/Prefix/GraphHop/Synonym/
  Paraphrase/MultiEntity); `run_golden` validates the dataset (errors on empty query/expected —
  never silently skips), scores per class, returns an `EvalReport` with a `passes(min_recall,
  min_mrr)` gate.
- **Refactored both evals onto the harness** (behavior-preserving): `regent-graph`'s
  `golden_retrieval` (same 12 cases, same 0.75/0.60 thresholds, now with per-class reporting) and
  `regent-embed`'s real-model `fusion_eval` (recall@3). One metric implementation, two crates.

**Verified:** `cargo test --workspace` green · clippy clean · `cargo test -p regent-embed --
--ignored` → paraphrase recall@3 **0.00 → 1.00** through the shared harness.

## 2026-06-16 — feat: P4 memory write-approval staging (§10.2 human gate) + daemon refactor

**Goal:** A human-approval gate for long-term memory writes — the agent *proposes*, nothing reaches
the graph until a person approves (master-prompt §10.2/§10.5). Per design doc §4.

**What was done (each slice tested green):**
- **Store** (`regent-store/infra/pending.rs`, schema v4→5): `pending_memory_writes` table +
  `enqueue` / `list` / `take` (atomic read-and-remove) / `delete_expired` (per-row TTL). 3 tests.
- **Graph staging** (`regent-graph/application/staging.rs`): `stage_write` (validated at stage
  time — injection/garbage never even queues), `pending_writes`, `approve_write` (commits via the
  normal node path → dedup + embedding), `reject_write`, `expire_pending_writes`. 3 tests, incl.
  injection-refused-at-stage-time.
- **Daemon + CLI:** RPC `memory.pending` / `memory.approve` / `memory.reject`; `regent memory
  pending|approve|reject`; hourly expiry loop (a missed decision auto-rejects, never commits).
- **Routing note:** the queue is the control plane; routing background-review writes through it
  (config-gated) is the clean follow-up — the memory *tool* writes the bounded MEMORY/USER stores,
  not graph nodes.

**Refactor (§3 file-size MUST):** `dispatcher.rs` (410) and `session_manager.rs` (397) split into
folder modules — `dispatcher/{mod,session_ops,admin_ops}.rs` and
`session_manager/{mod,build,hooks,queries}.rs`, all ≤176 lines, behavior-preserving (child modules
reach parent-private fields/methods via `pub(super)`).

**Verified:** `cargo test --workspace` green (21 daemon tests) · clippy clean · `go build/vet` clean.

## 2026-06-16 — feat: P4 tri-modal memory (Graph + FTS5 + Vector), local ONNX embeddings

**Goal:** Fuse three retrieval lanes — graph 1-hop, FTS5 lexical, and a new semantic vector lane —
into one ranker that beats the FTS-only pipeline (and Hermes) on paraphrase recall and token
efficiency. Local-first, zero per-query cost. (User directive overriding the design's conditional
embedding gate.) See **ADR-013**.

**Result (measured, real model):** paraphrase recall@3 — **FTS+graph 0.00 → tri-modal 1.00**
(`cargo test -p regent-embed -- --ignored`, all-MiniLM-L6-v2).

**Slices (each tested green before the next):**
1. **Store vector lane** (`regent-store/infra/embeddings.rs`, schema v3→4): `node_embeddings`
   table (f32 BLOBs, `model_id`-keyed, `ON DELETE CASCADE`); `upsert_embedding`,
   brute-force-cosine `vector_search` (sub-ms at personal scale — no C ANN extension),
   `nodes_needing_embedding` backfill list. 5 tests.
2. **Embedding contract + generator:** kernel `EmbeddingProvider` trait; `regent-graph` embeds on
   node write + `backfill_embeddings` (best-effort — a model hiccup never loses a memory); new
   **`regent-embed`** crate wrapping `fastembed` (ONNX, all-MiniLM-L6-v2, 384-dim) behind the
   trait, offline after first download. 3 graph tests + 1 ignored real-model test.
3. **Fusion** (`regent-graph/application/retrieve.rs`): lexical + vector seed lanes merged by
   weighted RRF (cross-lane agreement accumulates), then graph 1-hop, then `trust × recency`.
   Additive — no embedder ⇒ original FTS+graph. 3 fusion tests (`tests/vector_fusion.rs`).
4. **Eval** (`regent-embed/tests/fusion_eval.rs`, ignored): recall@3 gate proving the vector lane
   lifts paraphrase recall over FTS-only.
5. **Daemon wiring + config:** composition root loads the embedder (graceful: model-load failure
   degrades to FTS+graph), attaches it to `GraphMemory`, backfills in the background;
   `memory.embeddings` config key (default on).

**7 memory types mapping:** the fused ranker is the External/Retrieval transport (tier 5) serving
the persistent tiers — Semantic (2), Episodic (3), Procedural (4) — into Working memory (1);
Prospective (7) stays in `regent-cron`; Parametric (6) is the model weights.

**Verified:** `cargo test --workspace` green · clippy clean · `cargo test -p regent-embed --
--ignored` → recall@3 0.00→1.00. **Deferred:** cross-encoder reranking (RRF+trust/recency is the
rerank; YAGNI until evals justify); ≥50-pair golden set (paraphrase superiority already proven).

## 2026-06-13 — feat: P2.3 model catalog + model.set + streaming failover

**What was done:**

- **Runtime model switching:** `SessionManager` now holds a `ProviderFactory` (`Fn(&str) ->
  Arc<dyn ChatProvider>`) + a mutable current model instead of a fixed provider. Each new session
  builds a provider for the current model. `set_model` switches it for **new** sessions only —
  existing sessions keep their model so their prompt cache stays valid (a mid-session switch would
  invalidate the cached prefix). The composition root builds the factory (capturing provider kind,
  key, base URL); the cron runner keeps a fixed default-model provider.
- **RPC surface:** `model.list` (catalog: Fable 5 / Opus 4.8 / Sonnet 4.6 / Haiku 4.5, with a
  `current` flag) and `model.set` (accepts any id — the catalog is a menu, not an allowlist).
- **CLI:** `regent model` (active), `regent model list` (catalog, `*` marks current),
  `regent model set <id>`.
- **`FallbackChat::complete_streaming`:** failover now preserves streaming — a provider is only
  abandoned if it fails *before emitting any delta* (once text reached the user, a mid-stream
  failure surfaces rather than duplicating output on another provider).

**Verified:** `cargo test --workspace` green (model.list/set test added) · clippy clean ·
`go build/vet` clean · CLI smoke: `model` / `model list` / `model set` all correct.

**Deferred — adaptive-thinking passthrough:** enabling Claude thinking requires capturing and
replaying thinking-block **signatures** on assistant turns to keep multi-turn tool use valid
(Anthropic 400s otherwise). The internal `ChatMessage` stores reasoning as plain text with no
signature slot, so this needs a kernel `ChatMessage` extension — tracked as a follow-up, not a flag.

## 2026-06-13 — feat: bubbletea TUI + half-block pixel banner

**Goal:** Build the real interactive TUI (deferred from P1.2, unblocked by P2.2 streaming) and fix
the banner so the wordmark reads as a crisp pixel grid.

**What was done:**

- **Banner redesign:** the "REGENT" wordmark is now a **half-block (`▀▄█`) pixel font** — a
  hand-authored 5×7 glyph set scaled 2× and rendered with the silver gradient. (A braille attempt
  rendered muddy because a 5×7 font doesn't align to braille's 2×4 cells; half-blocks map one
  source pixel per cell, so letters stay legible and width-stable in every terminal.)
- **`shared/ui` split (architecture):** `ui.go` keeps the palette + panel/label helpers; the
  braille/half-block rasteriser, the king mark, and the banner moved to `shared/ui/art.go`.
- **bubbletea chat** (`features/chat/{chat.go,view.go}`): scrollable transcript (viewport),
  persistent input box (textinput), thinking spinner, live-typed replies from `message.delta`,
  tool-activity lines, inline y/N approval, Ctrl-C → `turn.interrupt`, `/quit` to exit. Daemon
  notifications/responses arrive as `tea.Msg`s through a re-issued `listen` command over
  `rpc.Client.Notifications`. Deps: `charmbracelet/bubbletea` v1.3.10 + `bubbles` v1.0.0.
- **`ui.EnableVT()`** moved to the cobra root so non-TUI subcommands keep ANSI on legacy Windows
  consoles (bubbletea manages its own terminal).

**Verified:** `go build/vet/test ./...` clean; banner render confirmed legible. Interactive TUI
needs a real TTY, so end-to-end click-through wasn't automated here.

**ADR:** ADR-012 amendment #2 updated — bubbletea adopted (was "deferred").

## 2026-06-13 — feat: P2.2 end-to-end streaming (SSE → message.delta → live CLI)

**Goal:** Stream assistant text token-by-token from the model all the way to the CLI, so replies
type out live. This is the path that makes a richer TUI (bubbletea) worthwhile — deferred in P1.2.

**What was done:**

- **`ChatProvider::complete_streaming`** (new trait method): invokes an `on_delta` callback per
  text fragment, returns the fully-accumulated response. Default impl is non-streaming (calls
  `complete`, emits once) so `OpenAiCompatChat` and scripted test providers satisfy it for free.
- **`AnthropicChat` SSE streaming** (`stream_once`): `"stream": true`, `reqwest` `bytes_stream`,
  newline-framed SSE parsing, single attempt (a partial stream can't be safely replayed).
- **`StreamAccumulator`** (pure, 2 tests): folds `message_start`/`content_block_*`/`message_delta`
  events into a `ChatResponse` — text deltas forwarded live, `input_json_delta` fragments
  reassembled into tool-call arguments, thinking deltas → reasoning, usage rolled up.
- **Anthropic adapter refactor (architecture):** the >200-line `anthropic_adapters.rs` split into
  `infra/anthropic/{request,response,stream}.rs` + `mod.rs`, each focused and under the size
  guideline (per the clean-architecture rule).
- **Agent delta sink:** `Agent::with_delta_sink(DeltaSink)`; when set, the turn loop calls
  `complete_streaming` (still inside the Ctrl-C `select!`), else `complete`.
- **Daemon wiring:** `SessionManager` attaches a sink that emits `message.delta` notifications
  (session id resolved from the same `OnceLock` cell as the tool/approval hooks).
- **CLI live render:** `regent chat` prints deltas as they arrive (silver, one open region),
  closes the region cleanly around tool-activity lines, and suppresses the final reprint when the
  reply already streamed.
- **Workspace:** added `reqwest` `stream` feature + `futures` to `regent-providers`.

**Verified:** `cargo test --workspace` green · clippy clean · `go build/vet` clean · E2E smoke in
Anthropic mode (dummy key) returns a graceful **401** through the streaming path — well-formed
request, clean error surfacing; real key needed only to see live tokens.

**Deferred (rest of P2):** bubbletea TUI (now unblocked by real deltas) · model catalog /
`model.set` · adaptive-thinking passthrough · Anthropic provider in the failover chain.

## 2026-06-13 — feat: P2.1 native Anthropic Messages provider + prompt-cache breakpoints

**Goal:** Begin P2 (loop/providers). Add a native `anthropic_messages` provider mode so Regent can
talk to Claude over the real Messages API (`POST /v1/messages`) instead of only OpenAI-compatible
endpoints — with prompt-cache breakpoints on the stable prefix, per the claude-api guidance.

**What was done:**

- **`regent-providers/infra/anthropic_adapters.rs`** (pure, 8 unit tests): translates Regent's
  OpenAI-style internal transcript ↔ the Anthropic block format.
  - Request: `system` as a separate cacheable text block; assistant `tool_calls` → `tool_use`
    blocks (arguments JSON-string → parsed object); tool results → `tool_result` blocks collapsed
    into one `user` turn so role alternation holds; `max_tokens` defaulted (Anthropic requires it).
  - **Cache breakpoints:** one `cache_control: {type:"ephemeral"}` on the last system block (or the
    last tool when there's no system) — render order is tools → system → messages, so this caches
    the entire stable tools+system prefix.
  - Response: `text`/`thinking`/`tool_use` blocks → content/reasoning/`ToolCall`; refusal
    `stop_reason` surfaces a placeholder instead of an empty turn; usage rolls
    `input + cache_read + cache_creation` into the prompt total.
- **`AnthropicChat` / `AnthropicChatConfig`** (`regent-providers/infra/implementations.rs`): raw-HTTP
  provider (no official Anthropic Rust SDK) with `x-api-key` + `anthropic-version` headers, default
  base `https://api.anthropic.com`, sharing `or-core` retry/backoff and the `ChatProvider` contract.
- **Daemon provider selection:** `ModelConfig.provider` (`ProviderKind`: `anthropic` default |
  `openai`), `REGENT_PROVIDER` env override; the composition root builds `AnthropicChat` or
  `OpenAiCompatChat` accordingly. Anthropic mode defaults the base URL to api.anthropic.com; openai
  mode keeps the openrouter default.

**Verified:** `cargo test --workspace` green (8 new adapter tests) · clippy clean.

**Deferred (rest of P2):** streaming (`messages.stream` → `message.delta` notifications, the
bubbletea trigger) · model catalog / `model.set` · adaptive thinking passthrough · provider failover
chain wiring for the Anthropic provider.

## 2026-06-13 — chore: relocate CLI to regent-cli/ + visual-identity polish

- **Folder rename:** `apps/cli/` → **`regent-cli/`** (repo root), per user directive; orphaned
  `apps/` tree removed. Go module path unchanged (`regent/cli`), so no import churn. Go tests
  re-verified green in the new location.
- **Visual identity rework** (`regent-cli/shared/ui/ui.go`):
  - Banner is now a **vertical silver gradient** (bright→dim across the 256-color grey ramp),
    matching the Hermes wordmark treatment in Regent's palette.
  - The kneeling-king mark is now **rasterised from vector strokes** (crown + bowed head +
    diagonal back + horizontal thigh + two separated legs with a triangular negative space) and
    **packed into braille** for a dotted pixel-grid look. Teal crown, uniform bright-silver body.
  - Panel outline switched to **silver** with the title set into the top border; width is measured
    ignoring ANSI codes so the right edge aligns on every row (fixes the earlier ragged border).
  - Session ID truncated in the panel to keep the TUI tidy.
- **bubbletea:** explicitly deferred to P2 (token-by-token streaming) — see ADR-012 amendment.
  P1.2 chat stays on the plain render loop.

## 2026-06-13 — feat: P1.2 Go CLI (`regent`) + visual identity + warm persona

**Goal:** The user-facing CLI plane (ADR-012): a Go binary that spawns `regent-daemon` as a
child process and speaks JSON-RPC 2.0 over stdio. Plus the user-mandated identity: Hermes-style
welcome screen with a "REGENT" pixel banner, a 2D pixel kneeling-king mark, silver/teal palette,
outlined info panel with bold/normal text mix, and a kind/thoughtful/warm agent persona with
light emoji use.

**What was done:**

- **Go toolchain**: go1.26.2 installed per-user (zip distribution → `~\.go-toolchain`; no admin).
- **`apps/cli/` Go module** (`regent/cli`, cobra v1.10.2), canonical clean-arch tree applied
  literally per ADR-012:
  - `shared/rpc/` — JSON-RPC client: `Spawn` (daemon child process over stdio),
    demux goroutine routing responses by id and fanning notifications onto a channel,
    `Call`/`CallAsync`. 3 unit tests against an in-process fake daemon (id routing,
    notification ordering, error surfacing).
  - `shared/daemon/` — daemon binary discovery (`REGENT_DAEMON_PATH` → CLI sibling → PATH →
    cargo target walk-up) and profile→home mapping (`-p name` → `~/.regent-profiles/<name>`;
    default honors `$REGENT_HOME`).
  - `shared/ui/` — the visual identity: teal/silver ANSI palette, "REGENT" pixel banner,
    kneeling-king pixel mark (teal crown, silver figure), outlined `Panel` with the title in
    the top border (visible-width aware around ANSI codes), bold `Header`/`Label` + normal
    `Note` text mixing, Windows VT enablement (stdlib syscall, no deps).
  - `features/chat/` — `regent` / `regent chat`: welcome screen (banner + outlined panel:
    king left, Session/Commands/Skills info right), prompt loop with teal `❯`, tool activity
    lines from `tool.start/complete`, inline y/N approval over `approval.request/respond`,
    Ctrl-C → `turn.interrupt` (never process exit), PowerShell-pipe BOM tolerated.
  - `features/sessions|cron|inspect|doctor` — `sessions list/search`, `cron list/add/remove`,
    `model`, `skills`, `config`, `doctor` (daemon binary, REGENT_HOME, API key warn,
    health + config.get round-trips), `version`.
- **Warm persona** — `BASE_PROMPT` in both composition roots (`regent-daemon` session manager,
  `regent-gateway` bin) rewritten: kind, thoughtful, warm, 1–3 well-placed emojis, capability
  and directness preserved underneath.
- **E2E verified**: `regent doctor` green against the real daemon (spawn → health → config.get
  → clean EOF drain); `regent chat` welcome screen renders the full identity and `/quit` exits.

**Verified:** `go build/vet/test ./...` clean (3 rpc tests) · `cargo test --workspace` 110/0 ·
clippy clean.

**Deferred:** bubbletea interactive render (lands with P2 streaming deltas — plain loop covers
P1 round-trip/approval/interrupt) · `sessions resume` into chat · skill slash commands in CLI ·
named-pipe attach mode.

## 2026-06-13 — feat: P1.1 regent-daemon crate (JSON-RPC 2.0 stdio server)

**Goal:** Implement the `regent-daemon` crate — the composition root that replaces the in-process
REPL with a long-lived JSON-RPC 2.0 process that any surface (Go CLI, Telegram gateway, future
TUI) can attach to over stdio.

**What was done:**

- `crates/regent-daemon/` — new workspace crate: 3-layer clean architecture (domain / application /
  infra), `bin/regent-daemon` binary.
- **Domain layer** (`src/domain/`):
  - `entities.rs` — `DaemonConfig` (additive serde defaults, `_config_version`), `RpcRequest`,
    `RpcResponse`, `RpcOutcome`, `RpcNotification`, `RpcErrorBody`, `ok_response`/`err_response`
    helpers, `ModelConfig`, `ContextConfig`, `MemoryConfig`, `CronConfig`.
  - `errors.rs` — `DaemonError` (From impls for `io::Error`, `serde_json`, `serde_yaml`,
    `RegentError`, `StoreError`).
  - `contracts.rs` — `OutboundTx = mpsc::UnboundedSender<String>`.
- **Application layer** (`src/application/`):
  - `session_manager.rs` — `SessionManager` (create/resume/run_turn/interrupt/resolve_approval/
    list/search/drain); `RpcApprovalHandler` (sends `approval.request` notification, blocks on
    oneshot, times out after 120 s → Deny); `SessionEntry` (Arc-per-session agent mutex +
    `CancellationToken` interrupt + approval oneshot).
  - `dispatcher.rs` — `Dispatcher` routes all v1 methods: `health`, `commands.list`,
    `session.create`, `session.resume`, `session.list`, `session.search`, `prompt.submit`
    (spawned task → `turn.started` + `message.complete` notifications), `turn.interrupt`,
    `approval.respond`.
- **Infra layer** (`src/infra/`):
  - `config_loader.rs` — `load_config(regent_home)`: reads/creates `config.yaml`, additive
    serde fill, version-mismatch warning; `expand_tilde` helper; 3 inline tests.
  - `transport.rs` — `StdioTransport` (async line reader over tokio stdin); `spawn_write_loop`
    (dedicated tokio task draining mpsc → stdout; eliminates stdout locking).
- **Composition root** (`src/bin/regent-daemon.rs`) — wires all 9 crates: config.yaml →
  store → graph → skills → provider → session_manager → dispatcher → stdio loop; cron tick
  loop; graceful shutdown drain; tracing writes to stderr (stdout is the JSON-RPC channel).
- **Additive store method**: `Store::list_sessions(limit)` added to `regent-store`.
- **Workspace changes**: `crates/regent-daemon` added to `[workspace] members`; `serde_yaml = "0.9"`
  and `tokio io-std` feature added to workspace deps.
- **Tests** (`tests/daemon_basics.rs`): 15 integration tests covering RPC type round-trips,
  session create/list/resume/run_turn/interrupt, approval no-op, and dispatcher routing
  (health, unknown method, session create + list, commands.list). All 105 workspace tests pass
  (87 pre-P1.1 + 18 new daemon tests).

**Gap closure (same day, after re-check against ADR-011 / p1-daemon-design.md):**

- **Methods added:** `skills.list` (via new `SessionManager::skills_list`), `model.get` (via
  `SessionManager::model`), `config.get` (config snapshot wired with `Dispatcher::with_config`),
  `cron.list` / `cron.add` / `cron.remove` (job repo wired with `Dispatcher::with_cron`;
  `Schedule::parse` errors surface as `-32602`).
- **Notifications completed:** `turn.complete` (success and non-interrupt failure) and
  `turn.interrupted` (matched on `RegentError::Interrupted`) now emitted by `prompt.submit`;
  `tool.start` / `tool.complete` emitted by a new `RpcToolHook` (`DispatchHook` impl) attached to
  every session catalog — the ADR-011 event surface the CLI renders as activity lines.
- **Config strictness:** `deny_unknown_fields` on every config struct — a typo'd key is now a
  hard load error, never a silent default (per p1-daemon-design.md).
- **Graph TTL purge loop** spawned in the bin (hourly, `spawn_blocking` off the runtime).
- New tests: config unknown-key rejection, model.get/skills.list, config.get round-trip,
  cron add→list→remove (+ bad-schedule error), prompt.submit notification stream order
  (`turn.started → message.complete → turn.complete → response`). **Workspace: 110 passed /
  0 failed; clippy clean.**

**Still deferred (by design, with phase homes):** named-pipe/socket attach transport (P1.2,
lands with the Go CLI's attach mode) · `model.set`/`config.set` (P2 — cache-aware model switch
starts a new session) · `clarify.request/respond` (P3, lands with the clarify tool) · curator
loop + episode-on-session-end (P4) · `regent doctor` `.env` lint (P1.2, it is a CLI command) ·
skill slash-command resolution (P1.2, CLI-side via `commands.list` + `skills.list`).

## 2026-06-12 — docs: P1 + P4 design documentation (CLI plane, daemon, memory/retrieval)

**Goal:** Pre-implementation design documentation for P1 (CLI plane) and P4 (memory/retrieval
completion). Crystallizes constraining decisions into ADRs and detailed design specs so that
implementation can proceed without revisiting architecture choices at each phase.

**What was done:**

- `docs/adr/ADR-011-daemon-json-rpc.md` — `regent-daemon` JSON-RPC 2.0 IPC design: two
  transport modes (stdio child-process + named pipe/socket attach); v1 method surface
  (`session.*`, `prompt.submit`, `model.*`, `config.*`, `skills.list`, `commands.list`,
  `cron.*`, `health`) + notification surface (`turn.*`, `tool.*`, `message.*`,
  `approval.*`, `clarify.*`) frozen at P1.3; single `config.yaml` loader with
  `_config_version` + additive reconcile (`.env` secrets-only; `regent doctor` lints
  behavioral `.env` keys); daemon-hosted loops (agents, cron, curator, TTL purge) with
  graceful shutdown drain; `regent-repl` retirement on P1.3 parity.
- `docs/adr/ADR-012-go-cli-plane.md` — Go CLI at `apps/cli/` applying the canonical
  clean-arch tree literally (cobra + bubbletea; `app/` root, `features/[subcommand]/`,
  `shared/` render primitives); streaming render contract (activity lines, inline approval
  modal, Ctrl-C → `turn.interrupt` over RPC); shared command registry from daemon
  (`commands.list` — CLI/gateway/TUI single source of truth); `-p <name>` profile
  isolation; long-tail subcommands ship with owning phase (no stubs in P1).
- `docs/p1-daemon-design.md` — `regent-daemon` crate internals: `domain/application/infra`
  layout (ADR-007 applied); transport-agnostic JSON-RPC dispatcher via two mpsc channels;
  `SessionEntry` lifecycle (`create/resume/interrupt/graceful-drain`); `config.yaml` schema
  skeleton + serde strict-mode + additive reconcile; full crate wiring table (which of the
  9 existing crates the composition root wires and how); `regent-repl` feature-parity
  checklist (the P1.3 gate — every REPL capability that must be reachable via `regent chat`
  before `regent-repl` is retired).
- `docs/p4-memory-retrieval-design.md` — Memory and retrieval completion: current M2 FTS5
  hybrid pipeline recap (OR-of-prefixes → BM25 seeds → 1-hop expansion → reciprocal-rank ×
  trust × recency); the embedding gate decision (sqlite-vec adopted only if paraphrase eval
  class drops below recall@5=0.75; test methodology and fusion design if gate triggers);
  golden set expansion to ≥50 pairs + trajectory eval format + gates; write-approval staging
  (`ApprovalQueue` domain contract, `pending_memory_writes` store table, TTL auto-reject);
  episode-on-session-end design for the P1 daemon's graceful-drain path.

**No code written, no builds executed.**

## 2026-06-12 — Hermes re-study (gap analysis) + full next-step roadmap

- `docs/hermes-study/10-gap-analysis.md` — post-M6 parity matrix against the full Hermes repo
  (84 tool files, 89 agent modules, ~30 CLI subcommands, 20+ platforms): done / partial /
  missing / deliberately-not-ported, each gap mapped to a phase.
- `docs/next-steps.md` — **the active roadmap** to complete Hermes parity in Regent's own
  architecture: P1 **CLI plane first** (regent-daemon JSON-RPC + Go `regent` CLI + single
  config.yaml loader + profiles), then P2 loop/providers (anthropic mode, streaming, catalog),
  P3 core tool parity, P4 memory/learning completion, P5 gateway breadth, P6 multi-agent
  (kanban, orchestrator delegation), P7 ops/security/CI, P8 ecosystem (mcp serve, TS surfaces,
  ACP). Includes the two Orchustr upstream windows (or-conduit tool-calls; or-colony caps) and
  standing rules binding every phase to the invariants ledger.

## 2026-06-12 — M6 edges: MCP via or-mcp, docker/ssh terminal backends, dispatch hooks

**Goal:** M6 per the proposal (§8): MCP client integration, sandbox backends, plugin seam.

**What was done (ADR-010):**

- **MCP integration** (`regent-tools/infra/mcp_tools.rs`) on Orchustr's **or-mcp**:
  `register_mcp_http(catalog, url, ns)` discovers a server's tools and registers them namespaced
  (`{ns}_{tool}`, toolset `mcp-{ns}`) with schemas carried into the model-facing definitions;
  dispatch round-trips through the client; upstream failures return as `{"error": …}` JSON;
  collisions reject loudly. Send-bound mismatch in or-mcp's native-async trait solved by boxing
  the invoker at the concrete client site (`McpInvoker`).
- **Terminal backends**: new `TerminalBackend` domain contract + `LocalBackend` /
  `DockerBackend` / `SshBackend` infra (CLI-shelling, pure unit-tested argv builders),
  `REGENT_TERMINAL_BACKEND` selection, `core_catalog_with_terminal()`. The terminal tool keeps
  guard/approval/truncation and reports its backend.
- **DispatchHook** observer seam on the catalog (before/after every executed dispatch, error
  results included; unknown tools/bad args never fire hooks).
- 6 new tests (MCP register/round-trip/failure/collision, hook observation, argv shapes,
  backend-env parsing).

**Verified:** `cargo test --workspace` → 87 passed / 0 failed; clippy clean; Rust 1.96.0.

## 2026-06-12 — M5 gateway: adapter contract, auth + pairing, /stop bypass, approval-over-chat, Telegram

**Goal:** M5 per the proposal (§8): the messaging surface with the Hermes invariants enforced in
harness code.

**What was done:**

- **New crate `regent-gateway`** (clean-arch internal; ADR-009):
  - domain: `MessageEvent`/`OutboundMessage`/`build_session_key` (the Hermes
    `agent:main:{platform}:{chat}` convention), the single **command registry** (`/help /new
    /stop /approve /deny /pair` + aliases; help text generated from it), `AuthPolicy` —
    default-deny evaluation (allow-all → allowlist → paired), one-time pairing codes; contracts:
    `PlatformAdapter` (pull) + `ConversationHandler` (agent side, cancellable).
  - application: `GatewayRunner` — dispatch order auth → commands → conversation; unknown users
    can only redeem pairing codes; one running turn per session with explicit busy reply;
    `/stop` cancels the in-flight turn (bypassing the busy guard); `/new` cancels + resets the
    session. `ApprovalRouter` + `ChatApprovalHandler`: dangerous tool actions prompt the chat and
    block on `/approve`//`/deny` with **deny on timeout** (never proceed by default).
  - infra: **Telegram adapter** — long-poll `getUpdates` with offset tracking, `sendMessage`;
    parse/build as pure unit-tested functions.
  - bin `regent-gateway`: full composition root — per-chat agents (graph memory, skills,
    delegation, background review, chat-bound approval handler), pairing state persisted to
    `gateway-auth.json`, operators from `REGENT_TELEGRAM_ALLOWED_USERS`.
- `Agent::reset_interrupt` — cancelled tokens re-arm per turn (long-lived gateway sessions).
- 10 new tests: command registry resolution/help, auth + pairing flow (deny → code → paired →
  round-trip), `/stop` bypasses busy guard and interrupts the turn (then guard releases),
  approval-over-chat approve path AND timeout-deny path, Telegram wire formats.

**Verified:** `cargo test --workspace` → 83 passed / 0 failed; clippy clean (one
guard-across-await restructured); Rust 1.96.0.

**M5 exit criteria status:** message round-trip ✅ (mock-adapter; live Telegram needs only a bot
token) · approval over chat ✅ · `/stop` bypasses guards ✅. Webhook/REST adapters deferred to the
daemon milestone (they belong with the HTTP/JSON-RPC listener).

## 2026-06-12 — Rust 1.96 upgrade + M4: cron (prospective memory) & delegation

**Goal:** Upgrade to latest stable Rust globally and in-project, then M4 per the proposal (§8):
`regent-cron` with the Hermes hardening invariants + parallel leaf delegation.

**What was done:**

- **Toolchain:** global rustup default 1.87 → **stable 1.96.0**; project pinned via new
  `rust-toolchain.toml` (clippy+rustfmt components); workspace `rust-version` bumped to 1.96.0
  (1.87 toolchain kept installed — Orchustr's checkout pins it). Fixed the three new 1.96 lints
  (two collapsed into now-stable let-chains, one checked division). 65/65 tests re-verified
  before M4 work began.
- **New crate `regent-cron`** (prospective memory, clean-arch internal; ADR-008):
  - domain: `Schedule` (`30m/2h/1d`, `daily HH:MM`, `@epoch` one-shot; parse + next-fire
    semantics unit-tested), `CronJob`, `JobRepository`/`JobRunner` contracts, RAII `TickGuard`.
  - application: `Scheduler::tick` — file tick lock (skip when held; stale lock broken after
    10 min), **hard timeout** per run (default 180 s; timed-out jobs still advance), catch-up
    clamp (period/2 ∈ [120 s, 2 h]; one-shot grace 120 s; missed-beyond-window → SkippedCatchup,
    never run late), one-shot retirement (disabled, never deleted).
  - infra: `FsJobRepository` (`jobs.json` + `.tick.lock` via atomic create_new).
  - 6 tests incl. the M4 exit criterion: due job fires exactly once under the tick lock; hard
    timeout aborts a 30 s runner in ~1 s.
- **Delegation** (`regent-agent/application/delegation.rs`): `delegate_task` tool — single goal
  or parallel `tasks[]` through `buffered(3)` (bounded + order-preserving), children are leaf
  agents (own session/budget 50, task brief + optional shared context only, leaf catalog without
  delegate/memory), per-child failure isolation. 2 tests incl. the M4 exit criterion: ordered
  results with a failing middle child isolated, each child in its own 2-row session.
- **`AgentJobRunner`** (`application/cron_runner.rs`): cron jobs run a fresh agent — source
  `cron`, no graph memory, no background review (the Hermes skip_memory rule).
- **REPL:** `delegate_task` registered (leaf catalog = core tools), cron scheduler loop spawned
  (30 s tick over `~/.regent/cron/jobs.json`, outcomes printed).
- or-colony adoption evaluated and deferred with reasons recorded (no concurrency cap,
  fail-fast aggregation) — ADR-008; upstream-then-adopt remains the path.

**Verified:** `cargo test --workspace` → 73 passed / 0 failed; clippy clean; Rust 1.96.0.

**M4 exit criteria status:** cron job fires once under tick lock w/ hard cap ✅ · parallel leaf
delegation returns ordered results ✅.

## 2026-06-12 — M3 learning loop + workspace-wide clean-architecture layout

**Goal:** M3 per the proposal (§8): skills loader + progressive disclosure + slash commands,
background review fork, curator + usage telemetry. Plus the user mandate: ALL crates follow
feature-based clean architecture internally (ADR-007).

**What was done:**

- **Clean-architecture migration (all 6 existing crates, behavior-preserving):**
  kernel → `types/` + `contracts/`; store/providers/tools/agent/graph → `domain/` +
  `application/` + `infra/` (entities + contracts + pure rules in domain; orchestrators/use
  cases in application; SQL/HTTP/process/fs in infra). Public APIs unchanged via lib.rs
  re-exports; `docs/architecture-mapping.md` updated with the layering contract.
- **New crate `regent-skills`** (procedural memory, agentskills.io-compatible, clean-arch from
  birth): `SkillRepository` contract (domain) + `FsSkillRepository` (infra: SKILL.md +
  hand-rolled frontmatter codec — no YAML dep — + `.usage.json` telemetry sidecar + `.archive/`);
  `SkillLibrary` use cases (progressive disclosure list→view→file with path containment,
  create/patch with hardline standards: name `[a-z0-9-_]`, description ≤60 chars ending with a
  period; archive refuses pinned); **curator** (`curate()`): agent-created + unpinned only,
  idle → stale → archive, never deletes; `REVIEW_SYSTEM_PROMPT` (versioned prompt).
- **Skill tools** in regent-tools/infra: `skills_list`, `skill_view` (full content, no
  pagination), `skill_manage` (create/patch/archive) via `register_skill_tools`.
- **Background review fork** (`regent-agent/application/review.rs`): after each successful turn,
  a whitelisted sub-agent (memory + skill tools only, max 8 iterations, source `review`,
  compression off, cannot recurse) reviews a conversation snapshot and persists learning.
  Fire-and-forget with a takeable JoinHandle for graceful shutdown/tests.
- **REPL**: skills library under `~/.regent/skills`, skills index in the frozen prompt (stable
  tier), skill **slash commands** (`/name task` → skill body injected as the user message,
  cache-safe, `record_use` telemetry), live learning loop enabled, review awaited on exit.
- New tests: skills library behavior (6 — disclosure, containment, hardline standards, patch
  telemetry, curator stale→archive with pinned/user immunity), frontmatter codec (2), learning
  loop (2 — review persists memory while the main conversation stays untouched; **agent-created
  skill persists & loads next session** = the M3 exit criterion).

**Verified:** `cargo test --workspace` → 65 passed / 0 failed; clippy clean.

**M3 exit criteria status:** skill created by agent persists & loads next session ✅ · curator
archives stale fixture skill ✅ (`library_behavior.rs`) · progressive disclosure + slash
commands ✅ · background review fork ✅.

## 2026-06-12 — M2 graph memory: nodes/edges/FTS5, bounded stores, hybrid retrieval, episodes

**Goal:** M2 per the proposal (§5/§8): native graph memory on SQLite + FTS5, the bounded `memory`
tool with Hermes semantics, recall tools, episode capture, and the cache-stability proof.

**What was done:**

- `regent-store` schema **v3**: `nodes` (kind, name, content, provenance, trust, session_id,
  TTL, access telemetry, unique `content_hash`), `edges` (unique src/dst/relation, weighted),
  `nodes_fts` FTS5 with sync triggers. New `graph.rs` persistence primitives: insert (idempotent
  by hash), find/by-kind, update/delete (edge cascade), upsert_edge, bidirectional neighbors,
  FTS match, access touch, TTL purge.
- New crate **`regent-graph`** (ADR-006): `GraphMemory` engine —
  - *Write policy*: injection-marker + invisible-unicode scanning, size caps, deterministic
    FNV-1a dedup hash scoped by kind+name.
  - *Provenance → trust*: user_stated 1.0 / agent_inferred 0.7 / tool_output 0.4 / web_content 0.3.
  - *Bounded prompt stores* (Hermes MEMORY/USER): add/replace/remove with unique-substring
    matching, hard char budgets (2,200 / 1,375) that error with current entries instead of
    auto-compacting, duplicate no-ops, `render_prompt_block()` frozen-snapshot rendering with
    usage headers and `§` delimiters.
  - *Hybrid retrieval*: OR-of-prefixes FTS5 query (stopword-stripped — fixed the implicit-AND
    zero-hit failure), BM25 seeds → bounded 1-hop expansion → reciprocal-rank × trust × recency
    scoring, access-telemetry touch, provenance-quoted "data, NOT instructions" rendering.
  - *Episodes*: `record_episode(session, summary)` anchor nodes.
- **Golden retrieval eval** (`tests/golden_retrieval.rs`): fixed knowledge graph + 12 query→
  expected pairs as a regression gate — **recall@5 = 1.00, MRR = 0.79** (gates 0.75 / 0.60);
  expansion-beats-lexical and telemetry tests alongside. Entry-semantics suite (6 tests) covers
  budget overflow with entries listed, replace-overflow, ambiguous/missing substrings, duplicate
  no-op, target isolation, snapshot format, and injection rejection at the boundary.
- `regent-tools`: `memory`, `memory_search`, `session_search` tools via `register_memory_tools`
  (catalog-registered like any tool; blocking graph calls bridged off the runtime).
- `regent-agent`: optional `with_graph_memory` — compression now records the evicted summary as
  an **episode node** tied to the parent session (recallable after the transcript is gone). New
  integration tests: memory writes mid-turn leave every API call's system prompt byte-identical
  while the write lands immediately and surfaces in the *next* session's snapshot; compression
  episode capture + retrieval.
- REPL: graph memory wired — snapshot block in the frozen prompt, memory toolset registered.

**Verified:** `cargo test --workspace` → 57 passed / 0 failed; clippy clean.

**M2 exit criteria status:** golden-set eval gates ✅ (recall@5 1.00 ≥ 0.75, MRR 0.79 ≥ 0.60) ·
cache-stability test (byte-identical prefix across turns) ✅ · memory tool budget semantics ✅ ·
session_search ✅ · frozen snapshot rendering ✅.

## 2026-06-12 — M1 hardened loop: fallback chain, compression + lineage, turn ledger

**Goal:** M1 per the proposal (§8): provider failover, run reproducibility, context compression.
Plus: TypeScript formally re-scoped to later surface work only (proposal amendment item 4 —
dashboard/desktop/optional Ink TUI at M5+, all JSON-RPC clients; never in the core path).

**What was done:**

- `regent-store` schema **v2**: `sessions.system_prompt` (frozen prompt persisted per session,
  added to old DBs by a new declarative column-reconcile pass), new `turns` table
  (model, api_calls, outcome, error, timestamps), `SessionMeta`/`TurnRecord` readers in new
  `meta.rs`, `record_turn`, `session_system_prompt`, public `now_epoch`. v1→v2 migration is purely
  additive and covered by a test that opens a hand-built v1 database.
- `regent-providers`: `FallbackChat` — ordered provider chain with **sticky, forward-only
  failover** on rate-limit/5xx/network/auth/retry-exhaustion; non-retryable 4xx surface
  immediately (they would fail identically everywhere). 3 chain tests.
- `regent-agent`:
  - **Context compression** (`compression.rs` + `lifecycle.rs`): preflight estimate (chars/4)
    against `trigger_fraction` × `max_context_tokens`; head summarized via one provider call;
    newest `protect_last_n` messages kept verbatim with a tool-pair-safe split; transcript rebuilt
    through invariant checks; **session split into a child** with `parent_session_id` lineage,
    parent ended with reason `compressed` (ADR-005).
  - **Turn ledger**: every `run_turn` records outcome (`ok`/`interrupted`/`budget_exhausted`/
    `error`), api-call count, model, and timestamps; recording failures log, never mask results.
  - **Resume correctness**: the stored system prompt now wins over the caller's fallback
    (byte-stability across resumes).
  - REPL: tracing-subscriber wired (`RUST_LOG` controls verbosity).
- New tests: compression E2E (split, lineage, end reason, tail verbatim, resume of child),
  mid-call interrupt (30 s provider cancelled at 50 ms → no partial history, ledger row
  `interrupted`), turns-ledger contents, fallback chain behaviors, v1→v2 reconcile.

**Verified:** `cargo test --workspace` → 44 passed / 0 failed; `cargo clippy --workspace
--all-targets` → clean.

**M1 exit criteria status:** interrupt mid-call ✅ · dangerous command requires approval ✅ (M0) ·
compressed session resumes ✅ · fallback chain ✅ · reproducibility ledger ✅.

## 2026-06-11 — M0 core implemented: Tokio-native Rust workspace on local Orchustr

**Goal:** Per user direction — use the local Orchustr checkout
(`D:\1-1@k\@ServeAI\Orchustr\orchustr`), replace the Node orchestration plane with Tokio
(ADR-001), and build the main core.

**What was done (each crate built + tested before the next):**

- `Cargo.toml`, `.gitignore` — cargo workspace (edition 2024, resolver 3), Orchustr `or-core` as a
  path dependency, all deps upper-bounded per supply-chain policy.
- `crates/regent-kernel` — `ChatMessage`/`ToolCall`/`Role`, `SessionId`/`TaskId`,
  `ToolDefinition` + JSON-string tool result helpers, typed `RegentError`, and `Transcript`,
  which enforces the Hermes alternation invariant by construction (ADR-004). 6 tests.
- `crates/regent-store` — SQLite via rusqlite bundled (ADR-003): WAL, `BEGIN IMMEDIATE`,
  jittered busy-retry (20–150 ms ×15), sessions/messages schema v1, FTS5 over
  content+tool_name+tool_calls with sync triggers, sanitized FTS query surface, session lineage
  column, usage accounting. 6 tests incl. on-disk round-trip and FTS search.
- `crates/regent-providers` — `ChatProvider` trait with **native tool calling** (or-conduit is
  text-only; ADR-002). `OpenAiCompatChat` for any chat-completions endpoint: payload building,
  parallel `tool_calls` parsing (string and object argument forms), reasoning capture, retry via
  `or-core` `RetryPolicy`/`BackoffStrategy` (429/5xx/network retry; auth/4xx fail fast). 5 tests.
- `crates/regent-tools` — explicit `ToolCatalog` manifest (duplicate-shadowing rejected,
  deterministic definition order, all errors wrapped to `{"error": ...}` JSON), dangerous-command
  guard routed through an `ApprovalHandler` gate (deny-by-default), and core tools: `terminal`
  (timeout + kill, output truncation), `read_file`/`write_file`, `search_files` (regex walk,
  skip-dirs, spawn_blocking). 12 tests incl. real process execution and approval-gate consult.
- `crates/regent-agent` — the turn loop: frozen system prompt, byte-stable tool schema list,
  harness-checked stop conditions (`max_iterations` 90, `CancellationToken` interrupt with
  abandoned-call semantics), parallel tool dispatch with call-order reattachment, per-message
  persistence + token usage accounting through one `spawn_blocking` seam, and `Agent::resume`
  replaying history through transcript validation. Plus `regent-repl` smoke binary
  (`REGENT_API_KEY`/`REGENT_MODEL`/`REGENT_BASE_URL`, stdin approval prompt). 4 E2E tests.
- `docs/adr/ADR-001..004` — Tokio-native decision, Orchustr adoption boundaries, rusqlite choice,
  transcript invariants.
- `docs/proposal/regent-architecture-v1.md` — v1.1 amendment block (two-plane architecture).

**Verified:** `cargo test --workspace` → 33 passed / 0 failed; `cargo clippy --workspace
--all-targets` → clean. Rust 1.87.0.

**Expected behavior:** `cargo run -p regent-agent --bin regent-repl` (with env vars set) gives a
working tool-using agent persisting to `~/.regent/state.db`.

## 2026-06-11 — Hermes study + Regent architecture proposal (docs only, no code)

**Goal:** (A) Study the Hermes Agent repository (`NousResearch/hermes-agent`, local copy under
`D:\1-1@k\1-1 Hermes Agent\`) and document how it works and interconnects; (B) propose the full
Regent rebuild architecture — TypeScript orchestration, Rust execution, Go CLI, Orchustr,
SQLite + FTS5, plus native graph memory.

**What was done:**

- `docs/hermes-study/README.md` — study index, Hermes summary, the two prime design principles.
- `docs/hermes-study/01-system-overview.md` — entry points, process topology, data flows, layout.
- `docs/hermes-study/02-agent-core.md` — AIAgent loop, 3 API modes, prompt tiers, compression,
  budgets/fallback, background self-improvement fork.
- `docs/hermes-study/03-tools-and-execution.md` — registry, toolsets, dispatch, approval flow,
  6 terminal backends, execute_code RPC sandbox, Footprint Ladder.
- `docs/hermes-study/04-memory-and-learning.md` — bounded memory, session search, skills,
  background review, curator, 8 memory-provider plugins.
- `docs/hermes-study/05-persistence-and-state.md` — SQLite schema v11, FTS5 (+trigram), lineage,
  write-contention policy, profiles, state inventory.
- `docs/hermes-study/06-gateway-and-surfaces.md` — gateway runner, 20 platform adapters, auth,
  TUI/desktop/dashboard/ACP surfaces.
- `docs/hermes-study/07-scheduling-and-delegation.md` — cron, delegate_task, kanban, the four
  concurrency mechanisms.
- `docs/hermes-study/08-extensibility.md` — four plugin systems, provider runtime, MCP,
  supply-chain policy.
- `docs/hermes-study/09-invariants-and-interconnections.md` — 25-point invariants ledger,
  interconnection map, warts to design away.
- `docs/proposal/regent-architecture-v1.md` — **PROPOSED** full build: three-plane topology
  (Go CLI ⇄ TS regentd ⇄ Rust crates via Orchustr), monorepo layout, Hermes→Regent subsystem
  parity matrix, graph-memory schema + hybrid FTS5 retrieval + eval gates, agent-turn GraphSpec,
  security model, phased plan M0–M6, risks, ADR seeds.

**Expected behavior:** documentation only — no code, no builds, nothing executed. Implementation
is gated on explicit approval ("go") of the proposal, starting at phase M0.
