# Changelog

## 2026-07-15 (b) — official Desktop installer script

**Goal:** the desktop app had no installer — only a manual dev build. Ship one
that builds the app and everything it needs, matching the CLI installers' style.

- `scripts/install-desktop.ps1` + `scripts/install-desktop.sh`: check prereqs
  (git/cargo/bun, with install URLs — never auto-install toolchains), build
  `regent-deacon` (release) into `~/.regent/bin`, pin `REGENT_DEACON_PATH` so
  the installed app — which lives outside the repo and can't reach `target/` —
  reliably finds the agent core, then `tauri build` the native installer
  (`.msi`/`.exe` · `.dmg`/`.app` · `.deb`/`.AppImage`) and print its path.
- Runs from a checkout or clones one to `~/.regent/src` (same
  `REGENT_REPO`/`REGENT_BIN_DIR`/`REGENT_SRC_DIR` overrides as the CLI
  installer). Windows `--run` launches the produced installer; Linux prints the
  extra WebKitGTK/GTK deps Tauri needs.
- Verified: both scripts parse clean; the load-bearing `cargo build --release
  -p regent-deacon` step produces the binary. Docs: README desktop note +
  `docs/development/desktop.md` now lead with the installer.

## 2026-07-15 — bug-backlog sweep: onboarding wizard, agent editors, honest Memory Home, usage-ledger fix, log forensics

**Goal:** work the owner's 2026-07-14 bug backlog (docs/plans/bug-backlog-2026-07-14.md)
plus the onboarding UX feedback, without breaking anything. Verified per change:
CLI `bun test` 56/56 + `tsc` clean + recompile; `cargo test -p regent-deacon -p
regent-skills` all green; Desktop `npm run build` clean. Three fresh-eyes review
loops over the full diff (loop 1 caught the welcome-name "I" regex edge; loop 2
verified style/RPC contracts; loop 3 re-ran every gate).

- **First-run wizard gate fixed (backlog #7):** any deacon-booting command
  (e.g. `regent model list`) seeded config.yaml and silently skipped onboarding
  forever. Gate is now a wizard-written `.setup-done` marker (or existing
  `.env`) — `features/setup/domain/firstRun.ts` + unit tests.
- **Interactive setup wizard (owner UX feedback):** bare `regent setup`/first
  run now opens an Ink TUI styled like the chat: REGENT banner header,
  arrow-key provider picker fed by the new deacon RPC **`providers.catalog`**
  (`ProviderKind::ALL` — all 18 kinds + curated models, single source of
  truth), per-provider model list with type-to-filter (unmatched text = custom
  id; free text for ollama), **masked** API-key input, a "Tell me about
  yourself" step saved to the `about` persona row with a personalized crowned
  welcome, review screen, Esc-goes-back everywhere. Base URL is flag-only
  (provider defaults live in the deacon). Linear prompt flow kept for
  flags/non-TTY/no-deacon; persistence shared in `domain/writeSetup.ts`.
- **Data-directory picker in onboarding:** a wizard stage chooses where
  `.regent` lives (config, keys, memory, skills). Persists via a one-line
  pointer at the default `~/.regent/.home` — the only place resolvable before
  config exists; `REGENT_HOME` env and `-p` profiles still win (the wizard
  warns when env would override). The CLI and the deacon it spawns share the
  resolution; `uninstall --purge` follows the pointer. This also answers
  backlog #6's open question about a first-class home setting.
- **`agents edit <name>` is now usable (backlog #1):** bare edit opens a
  field-by-field editor (description/prompt/model/tools, Enter keeps, Y/n
  confirm) instead of silently re-saving; `agents show` prints multi-line
  prompts as a block. Skills-per-agent deferred (needs store+dispatch support).
- **Desktop Agents settings page (backlog #2):** new section — agent rail,
  full editor (name/description/system prompt/model/tools), save via
  `agents.set`, two-step delete, `+ New agent`.
- **Memory Home told honestly (backlog #6):** `memory.home` can never take
  effect (the deacon resolves REGENT_HOME before config.yaml is read), so the
  app now explains how to actually move it, `regent config set memory.home`
  errors with the same guidance, and `memory.embeddings` became a real
  editable toggle in Workspace.
- **Skills usage ledger no longer corrupts (backlog #4 contributor):**
  `.usage.json` was torn by concurrent deacons' in-place writes, and the reset
  fallback wiped all usage telemetry on nearly every boot. Atomic temp→rename now.
- **Chat cutoffs diagnosed from logs (backlog #3):** `api_calls=91` wrap-ups =
  main chat exhausting `max_iterations` 90 (by design, needs a Desktop
  "Continue" chip — fix path recorded in the backlog with the
  `budget_exhausted` surface to expose); `api_calls=9` = review sessions'
  8-cap; compaction circuit-breaker session-splits observed right before one
  cutoff. #4 (recall) partially audited: local memory writes are NOT
  approval-staged, retrieval evals pass — capture-side instrumentation is the
  next step. #5 (doc-forge) has no failure evidence in logs; needs live repro.

## 2026-07-14 (f) — new Regent app icon

**Goal:** replace the placeholder desktop icon with the real Regent mark.

- New source art at `assets/logo/MAIN/icon.png` (1024×1024) + `icon.ico`.
- Regenerated the full Tauri icon set with `tauri icon` — every
  `src-tauri/icons/` size (32/64/128/128@2x, `.ico`, `.icns`, the Windows
  `Square*`/`StoreLogo` set) plus the iOS `AppIcon-*` and Android
  `mipmap-*` launcher sets. `tauri.conf.json` already pointed at these paths,
  so no config change.
- Windows embeds the icon into the exe at compile time; `cargo clean -p
  regent-desktop` cleared the stale build so the next `tauri dev`/`build`
  re-embeds the new mark.

## 2026-07-14 (e) — auto mode (approve everything) + write_file stops popping Explorer

**Goal:** a user-toggleable auto mode for the coding path that approves every
tool gate, settable from both the desktop app and the CLI — and kill the
annoyance where every file the coder created popped a File Explorer window.

**Auto mode (config `tools.auto_approve`, default off):** the core `AllowAll`
handler already existed but was reachable only via `REGENT_AUTO_APPROVE`. Now:

- `ToolsConfig.auto_approve` (deacon `domain/config/runtime.rs`) — a real
  config field, so `regent config set tools.auto_approve true` works and the
  `config.set` whole-file validation gate applies.
- `ConfigGatedApprover` (`session_manager/session_ctx.rs`) wraps the RPC
  prompt handler and checks a shared `AtomicBool` PER REQUEST — the
  dispatcher's `apply_config` stores the flag on every `config.set`, so the
  toggle reaches OPEN sessions instantly, both directions. `ask_user` is
  exempt: auto mode skips permission prompts, it does not answer the agent's
  questions (unlike the headless voice env-var path, a human is present).
  4 unit tests (`session_ctx_tests.rs`). Env `REGENT_AUTO_APPROVE` unchanged;
  gateway/MCP/cron surfaces keep their own defaults (prompt / DenyAll).
- Desktop: new Settings → **Code** page (`CodeSection.tsx`) with the toggle
  bound through the generic `useConfig`/`ConfigField` engine; Safety is
  sandbox-only again and honest about it. Nav + search keywords registered.
- CLI: `regent code settings` shows code settings; `regent code settings auto
  on|off` flips the flag via the validated `config.set` RPC (applies live).
  Listed in `regent help`.
- CLI bug fixed along the way: `regent code --yes` only skipped the
  client-side plan confirm — in-run `approval.request` notifications
  (dangerous shell, move/copy/delete, ask_user) had NO subscriber and stalled
  ~120 s server-side before auto-denying. `--yes` now subscribes before
  `code.plan`, auto-answers approvals for this command's own sessions only
  (ids learned from `session.created`/`code.started`; the one-shot spawns a
  dedicated deacon), prints a grey "auto-approved: …" notice per answer, and
  unsubscribes in `finally`.

**Explorer-popping fix:** `WriteFileTool` (`regent-tools/infra/files.rs`) no
longer calls `reveal()` for brand-new files — a coding run creates many files
and each popped an `explorer /select` window. Generated images keep their
reveal (rare, deliberately user-facing, still `REGENT_REVEAL_FILES`-gated and
throttled).

**Verified:** workspace `cargo test` 0 failures, clippy 0 warnings, fmt clean;
CLI `bun run typecheck` + `bun test` 46 pass / 0 fail + biome clean on touched
files; desktop `tsc --noEmit` clean. Three review passes (found + fixed:
`applyLabel` misuse, mid-file test hook, section keyword drift).

## 2026-07-14 (d) — structural 200-line sweep, tranche 1: 28 files split, all verified per-commit

**Goal:** the user-ordered structural sweep of every file past the 200-line
rule, WITHOUT breaking anything. Method (proven safe over 15 commits): move
whole items to a sibling module opening `use super::*;` (child modules see
parent privates), parent re-exports moved pub types so every existing path
stays valid, visibility widened only where the compiler demands
(`pub(super)`/`pub(crate)`), `cargo check` → crate tests → commit per file.
Line-parity vs HEAD verified on the mechanical moves.

**Split (before → after, new modules):** graph orchestrators 497→186
(+semantic/views/tests); deacon queries 429→148 (+board_queries/
memory_queries), build 438→~185 (+prompt_lines/catalogs), bin 430→186/176/141
(regent-deacon/{main,routing,boot}.rs, bin path updated, live RPC
smoke-tested), session_ops 400→162 (+prompt_ops/turn_errors), model_ops
333→111 (+providers_ops/support), env_ops 306→198 (+env_catalog),
dispatcher/mod 283→167 (+wiring/catalog_data), webhook 349→118
(+inbound/delivery), registry 266→176 (+registry_ext), webhook/tests
351→213+144, sm lifecycle 277→124 (+session_ctx), sm mod 275→240
(+turn_meta), provider_catalog 268→70 (+model_lists data table 203);
tools memory_tools 379→152 (+actions/session_tools), catalog 322→178
(+tiering), play 287→103 (+resolve); agent turn 364→279
(+turn_support/wrap_up), agent/mod 317→182 (+resume/telemetry); gateway bin
372→99/198/89; store graph 286→191 (+graph_edges), sessions 278→168
(+session_messages); skills library 275→208 (+index_render); voice turn
327→228 (+synth), deacon 272→180 (+stream); code harness 273→230 (+prompts);
vendored paddle-ocr-rs lib tests → tests.rs.

**Documented exceptions (cohesion beats the number):** agent turn.rs 279
(run_turn IS the loop), harness.rs 230 (the phase spine), voice turn.rs 228
(one call turn), model_lists.rs 203 + skills library 208 + webhook tests 213
(data/test tables). Vendored logic (paddle-ocr db_net/ocr_lite, or-mcp
multi_client) exempt — restructuring third-party inference code is risk with
zero functional gain.

**Tranche 2 (same session, commits →622beb9):** kernel speech 255→124
(+speech_types re-exported), computer_use 245→203 (+parse), web_search
239→152 (+web_fetch), agent lifecycle 242→168 (+history_levers), tools
contracts 235→173 (+permissions re-exported), feishu 262→186 (+files).
34 files split total across 21 commits; checkpoint after each and at the
end: workspace tests exit 0 (68 suites), clippy 0, fmt clean.
key_tool/catalog.rs (241) joins model_lists.rs as a data-table exception.
**Remaining (~10 borderline 210-250 + big test files):** whatsapp/discord
249, wecom 244, slack 228, store schema 227, discord_interactions 223,
gateway contracts 222, openai_stream 219, gateway runner 219, ledger 218;
tests library_behavior 327, gateway_behavior 265, learning_loop 254,
store_roundtrip 253, deacon_basics 249/238/231, golden_retrieval 235.

**Verified:** full `cargo test --workspace` exit 0 (68 suites), clippy 0
warnings, fmt clean — re-run after every single split; deacon additionally
smoke-tested live over stdio RPC after its bin restructure.

## 2026-07-14 (c) — zero clippy warnings: Problems-panel cleanup

**Goal:** user asked for the editor's yellow/red marks gone. The red ~2K were
stale rust-analyzer state after the vendored workspace member landed (cargo
builds green; fixed by restarting the RA server / reloading the window). The
yellows were 18 real clippy warnings — all fixed, workspace now at **0**:

- `doc_lazy_continuation` ×12 — doc-comment list continuations indented (or
  reflowed to a paragraph): camera.rs, config_ops.rs, model_ops.rs,
  backfill.rs, orchestrators.rs.
- `items_after_test_module` ×3 — mid-file `mod tests` blocks moved to sibling
  files (test-count parity verified against HEAD: 1/2/1 moved, 0 lost):
  store `db.rs` → `db_tests.rs`, tools `memory_tools.rs` (421→~370 lines) →
  `memory_tools_tests.rs`, `search_providers/mod.rs` → `tests.rs`.
- Semantics-preserving one-liners: `repeat_n` (fence.rs), `split_once`
  (key_tool/env_file.rs), `sort_by_key(Reverse)` (token_budget.rs test),
  let-chain collapse (artifacts_ops.rs), `into_iter` removal (vendored
  ocr_lite.rs).

**Verified:** `cargo clippy --workspace --all-targets` = 0 warnings,
`cargo fmt --check` clean, full `cargo test --workspace` exit 0.

## 2026-07-14 (b) — review pass over Waves 1–4, PaddleOCR rung for read_document, banner-trail fix, security-audit hardening

**Goal:** user-ordered sequence: review every shipped Wave 1–4 change for
correctness of logic and flow, fix what the review and new bug reports
surfaced, then add local OCR so scanned/image documents actually read.
Verified three times over (full workspace tests ×3, clippy, fmt, CLI bun
tests + tsc, desktop tsc — all green each pass).

### Review findings → fixes
- **CLI banner trail + exit duplication (user-reported)** — the deacon was
  spawned with `stderr: "inherit"`, so its boot/drain logs wrote into the
  terminal mid-Ink-render; every foreign line desyncs Ink's frame erase
  (stacked partial banners at launch, duplicated chat input at exit). The
  deacon's stderr is now DISCARDED by default (its redacted rolling file under
  `~/.regent/logs/` already has everything); `REGENT_LOG=…` restores terminal
  streaming. `regent` binary rebuilt + relinked. (`spawn.ts`)
- **`export_vision_route` marker was global, not per-var** — after boot 1 set
  the marker, a config reload would clobber a genuinely user-set
  `REGENT_VISION_*` var. The marker now stores the exact var names this
  function exported; only those refresh. Also: switching to a provider with no
  OpenAI-style route (Anthropic / keyless) now CLEARS our exported vars so
  documents never keep flowing to the provider the user left. (`regent-deacon.rs`)
- **`regent security audit` false positives/negatives (user-asked)** —
  (a) the secret lint flagged `providers.*.api_key_env` (an env-var NAME, the
  healthy pattern) — now `_env`-suffixed keys and UPPER_SNAKE reference values
  are exempt; (b) key presence only checked `REGENT_API_KEY` — now the union
  of that plus every configured `api_key_env`, in the environment or `.env`.
  Helpers exported + 5 bun tests (`securityCommand.test.ts`).
- **read_document polish** — the model-direct skip reason now also rides the
  local-extraction ERROR path (both rungs' failures named in one message);
  non-JSON provider error bodies include the HTTP status; `content` returned
  as an array of parts (some providers) no longer reads as an empty reply.
- **Doom-loop nudge wording** — no longer claims "identical results" (only
  name+arguments are compared); steering text shouldn't lie to the model.
- Reviewed clean: retry-after backoff, compaction breaker (completes the
  in-flight split then locks), ask_user/approval plumbing (empty feedback
  degrades to Deny), shell hooks, mid-tier collapse, truncation spill,
  permission rules, partitioned dispatch, budget wrap-up, fix-retry loop,
  explore scout, todo tool, diagnostics.

### PaddleOCR rung (read_document ladder rung 4)
- When extraction yields near-empty text (<200 chars — scanned PDF, photo
  deck), the document's images are OCR'd **locally** with the official
  PP-OCRv4 det/cls/rec models (RapidOCR's ONNX exports of the same weights —
  ONNX is what lets them run on the `ort` runtime fastembed already ships;
  no Paddle C++ stack). Models (~16 MB) download once from Hugging Face into
  `~/.regent/models/ocr` (temp-then-rename; size floor against error pages).
- PDF page images come from a new lopdf-based `/Image` XObject extractor
  (JPEG verbatim, Flate 8-bit RGB/Gray rebuilt; JBIG2/CCITT skipped with a
  count; largest-first, 12-image cap). OOXML media reuses the existing rels
  extraction. OCR text replaces the thin text with `source: "local-ocr"`;
  every failure degrades to an `ocr: skipped …` field, never a failed read.
- `paddle-ocr-rs` 0.6.1 **vendored** at `src/crates/paddle-ocr-rs` (Apache-2.0,
  attribution in its Cargo.toml): upstream pins ort rc.10/ndarray 0.16 which
  cannot coexist with fastembed's ort rc.12/ndarray 0.17 — pins bumped and the
  four rc.12 API drifts fixed (builder error type folds, `inputs()`/`name()`
  methods, `custom()` metadata shape); upstream fixture tests marked ignored.
  OCR runs under `spawn_blocking` (panic-isolated), engine cached in a
  `OnceLock<Mutex<…>>`.
- **Live-verified**: rendered a text PNG, ran the ignored e2e —
  `OCR read: Regent reads scanned pages 2026`. Blank-page path verified too.
- Doc deps bumped to latest per user: zip 8, calamine 0.36, pdf-extract 0.12,
  lopdf 0.44 (+ image 0.25). documents SKILL.md teaches the full ladder.

### Editor lint noise (user-reported)
- Root `.markdownlint.json` + `.vscode/settings.json` silence line-length
  squiggles (MD013/MD033/MD041; ruff/pylint/flake8 line length) across docs,
  python-voice-server, and the app/web folders. Repo discipline is file
  length, not line width.

### File-size sweep (tranche 2, mechanical)
- 30 more files' trailing `#[cfg(test)]` mods extracted to sibling
  `*_tests.rs` via `#[path]` (delegation/tool, mom/mod, constitution,
  diagnostics, titling, provider_kind, 9 gateway platforms, evals, transcript,
  anthropic request/response, openai_compat, realtime lib, speech registry,
  store embeddings/kanban/persona, tools backends/checkpoint/control_app/
  files/file_ops/mcp_server/mcp_tools/sandbox/terminal/video_analyze/
  vision_analyze/web_search, voice vad). read_document/mod.rs split
  structurally (`extractors.rs`). Remaining >200-line files needing REAL
  structural splits (~30, e.g. orchestrators.rs 497, session_manager/build.rs
  438, queries.rs 429, memory_tools.rs 421, agent/turn.rs 364) are deferred —
  mechanical splits of live dispatch code risk the breakage this session was
  told to avoid. Vendored crates exempt by policy.

### Verified
- 3× full `cargo test --workspace` (exit 0 each), `cargo clippy` (remaining
  warnings only in parallel-session-owned files: backfill.rs, key_tool/
  env_file.rs, fence.rs), `cargo fmt --check` clean, CLI 46 bun tests + tsc
  clean, desktop tsc clean, deacon + CLI binaries rebuilt.
- NOT verified live: a real scanned PDF through the full deacon → read_document
  → OCR path (needs a session with such a file); butler carve-out still needs
  a voice-server restart.

## 2026-07-14 — regent-code v2 Wave 4 + six bug hunts: retry-after, compaction breaker, ask_user, shell hooks, documents, butler routing, MoM/agents CLI, research skill, desktop slash parity, file-size sweep

**Goal:** finish the plan's Wave 4 (P3) and fix the user-reported bug list of
2026-07-13/14, without breaking anything (whole workspace green throughout).

### Wave 4 (P3)
- **4a — `retry-after`-aware backoff**: `ProviderError::RateLimited` carries
  `retry_after_ms` (parsed numeric-seconds header at all four 429 sites);
  `run_with_retry` sleeps the server-stated delay (capped at `max_delay_ms`)
  instead of the jittered guess. Tests: header parsing + delay honored.
- **4b — compaction circuit breaker (gap C4)**: a compression pass that fails
  to bring the estimate back under threshold opens `compression_broken` for
  the session (exactly one split, never a split loop) + `tokens_before/after`
  telemetry on every split. Test: `ineffective_compaction_opens_the_circuit_breaker`.
- **4c — `ask_user` tool (gap T4)**: one blocking structured question riding
  the existing approval channel end-to-end. `approval.respond` gained an
  additive `feedback` string; the deacon oneshot is now `(bool, Option<String>)`;
  `DenyWithFeedback` finally has a live RPC producer. In the CLI chat, any
  non-affirmative reply to an approval IS the feedback/answer. Registered for
  code sessions only (chat has the human; SPL catalog gate).
- **4d — lifecycle shell hooks (gap S7, observe-only)**: `tools.hook_tool_start`
  / `tools.hook_tool_complete` config commands spawn fire-and-forget at the
  dispatch seams with `REGENT_HOOK_EVENT/TOOL/PAYLOAD` env (Windows `raw_arg`
  so redirects survive). Blocking pre-hooks deliberately skipped — permission
  rules (3a) own gating. Skipped per the plan's own conditions: `lsp` tool
  (heavyweight), per-model prompts (telemetry-gated).

### Bug hunts (user-reported)
- **Documents (bug 1)** — root cause from sess_7d79…: no document tool, and
  Windows `python3` is a store shim that hangs and exits 0 with no output.
  New `read_document` tool (deferred by default): ladder = (1) model-direct
  PDF read via OpenAI-compatible `file` part — provider failures surface as a
  NAMED `model_direct: skipped: …` reason, e.g. "…likely doesn't accept
  file/document inputs", never a silent downgrade; (2) native extraction
  (pdf-extract / OOXML strip / calamine) + embedded images extracted to the
  scratch area for `vision_analyze` + hyperlinks from rels; (3) text-only.
  New deps: zip 0.6 (already in lock), calamine, pdf-extract. The
  `REGENT_VISION_*` fallbacks now FOLLOW the active provider (exported from
  routing at boot + config reload; user-set values always win; Anthropic
  exports nothing) — fixes the "static provider" complaint. New bundled
  `documents` skill: creation recipes (Edge headless HTML→PDF, OOXML zip,
  python-docx/pptx where python is VERIFIED) + the python3-shim trap.
- **Butler diagram instead of code task (bug 2)** — `VISUAL_EXPLAINER`'s
  override list covered search/browse/screen but not WORK requests; added the
  action carve-out (code_task/kanban/delegate/background_task/terminal/
  send_message) + regression test. Applies after a voice-server restart.
- **MoM setup (bug 3)** — the CLI existed but was buried: new top-level
  `regent mom` (alias `agent`), in help/groups/slash-picker, and the agent's
  CAPABILITIES now teach `mom create … --proposers … --aggregator …` and
  `/mom run <name> "<brief>"`.
- **Agent creation (bug 4)** — backend was complete (`agents.*`); the real
  bug: in-chat commands split on whitespace with NO quote handling, so any
  create-style command with quoted args was mangled. `runChatCommand` gained
  a shell-style tokenizer (+ tests).
- **Research skill (bug 5)** — new bundled `research` skill (Hermes port):
  sweep→read→verify→cite method, the 16 user-listed primary scholarly
  sources, and 21 more source families. Bundled skills now: ponytail,
  code-reviewer, secure-code-guardian, documents, research.
- **Desktop slash parity (bug 6)** — the app chat now executes EVERY known
  command as direct deacon RPCs (subcommand-aware, quote-honoring tokenizer:
  agents create/edit/remove, kanban verbs, memory verbs, skills view/opt,
  mom list/run, config get/set, env, voice, providers models/test, persona);
  terminal-only commands answer with guidance. Slash commands never reach the
  model anymore.

### File-size sweep (task 7, partial)
24 oversized files' trailing test mods extracted to sibling `<stem>_tests.rs`
via `#[path]` (verbatim moves; behavior identical; whole workspace green).
Oversized count 48 → 28; the remainder (session_manager build/queries,
dispatcher session_ops/model_ops, agent turn/mod, graph orchestrators,
memory_tools, catalog.rs, webhook, both bins, voice turn/deacon …) need real
structural splits — deliberately deferred, listed here as the follow-up.

**Verified:** full workspace `cargo test` green, fmt clean, clippy clean (new
code), CLI 41 bun tests + tsc clean, desktop tsc clean, `regent mom` /
`regent agents mom` smoke-tested against real config. NOT verified live: a
real `regent code` dogfood run, butler voice-call behavior (needs voice-server
restart), and a real PDF through the model-direct rung (needs credit).

**Goal:** the plan's P1 and P2 waves ([docs/plans/regent-code-v2.md](plans/regent-code-v2.md)
§Wave 2–3), same session as Wave 1.

- **2a — fix-retry on red verify (gap H4)**: a red verify now feeds its failure
  output back to the SAME execute agent/session as a fix turn (`fix_prompt`),
  bounded at 2 attempts, before the unchanged revert backstop. Implemented in
  BOTH `CodeHarness::run` (agent kept alive across attempts;
  `with_max_fix_attempts(0)` restores one-shot) and the live deacon
  `code.start` loop. `CodeOutcome`/`CodeStartResult`/RPC/`code_task` carry
  `fix_attempts`; the CLI prints "(after N fix attempt(s))".
  `regent-code/tests/fix_retry.rs`: red→green = 1 fix turn + no revert;
  red×3 = revert.
- **2b — doom-loop guard (gap L1)**: the third identical single-call batch in
  a row is not dispatched — a synthetic steering result comes back instead
  ("…3 times in a row… change your approach"). Window stays saturated, so a
  stubborn loop keeps getting nudged and converges to 2c's wrap-up.
  `regent-agent/tests/doom_loop.rs` proves the tool runs exactly twice.
- **2c — graceful budget exhaustion (gap L2)**: `max_iterations`/token-ceiling
  no longer return `Err(BudgetExhausted)` — one final TOOL-LESS model call
  (`WRAP_UP_PROMPT`) returns done/remaining/where-to-resume as `Ok`, streams
  to the delta sink, drops stray tool calls, and the turns ledger still
  records `budget_exhausted` (flag read by `record_turn_outcome`). Existing
  budget tests updated to the new contract.
- **2d — tool-output truncation with spill receipt (gap T6)**: central in
  `ToolCatalog::dispatch` — results past 30k chars spill in full to
  `ToolContext::scratch_dir` (`<seq>-<tool>.txt`) and the model gets the head
  plus "[truncated — full output at <path>…]"; no scratch dir → head only,
  never an error. The deacon sets the scratch to
  `$REGENT_HOME/artifacts/tool-output` (inside the jail's allowed subtree).
- **2e — explore scout (gap T3)**: new `explore` tool (question + optional
  context) → `SessionManager::run_explore`: a fresh agent on the plan-mode
  read-only catalog subset (explore itself excluded — no recursion),
  `EXPLORE_PROMPT`, 15 iterations / 60k tokens, source `explore`. Parent
  transcript grows by ONE tool result; child session persists.
  `deacon_basics/explore.rs` proves both. `code_task`'s description was
  trimmed again to keep the resident catalog under the 2.2k SPL gate (each
  deferred tool adds a `load_tools` hook line).
- **3a — permission rules as data (gaps S5/S6)**: `PermissionRule
  { permission, pattern, action: Allow|Ask|Deny, feedback }` with
  last-match-wins `*`-wildcard evaluation over the call's subject
  (path/command/url → raw args), carried on `ToolContext` and consulted in
  `dispatch`. Deny returns its feedback AS the tool result; Ask routes
  through the existing `ApprovalHandler`. `ApprovalDecision` gained
  `DenyWithFeedback(String)`; every denial site now uses fail-closed
  `.denied()` (a non-Approve variant can never slip through as approval).
  Scope note: mechanism + wiring only — no rules ship by default (empty =
  behavior unchanged), and the existing terminal jail / sandbox were NOT
  migrated to rules (defense in depth stays physical); config-driven rules
  are the natural follow-up.
- **3b — todo tool (gap T2)**: `todo_write` (full-list replace, rendered
  echo). Registered for CODE-EXECUTE sessions only — chat has `kanban`, and a
  chat registration would re-trip the catalog token gate. Per-session
  in-memory rather than the plan's shared `todos.json` (which would
  cross-clobber concurrent sessions).
- **3c — failing-test-first (gap H6)**: plan prompt now requires a bug-fix
  plan to open with a failing repro test. The plan's repro-test verify fast
  path was NOT built (no reliable filter source in plan metadata yet).
- **3d — mid-tier collapse (gap C3)**: new `domain/collapse.rs` — stale tool
  exchanges lose their fat tool-call ARGUMENTS (`write_file` bodies, patches;
  the half result-pruning never reclaims), stub is valid JSON, ids/roles
  legal, protected tail absolute, same 2k-token batch floor; staleness = 2×
  the pruning horizon so the tiers stay ordered. Runs between `maybe_prune`
  and `maybe_compress` in the turn loop.
- **3e — review phases (rides 1c)**: `regent code --review <skill>`
  (repeatable) / `code.start` `review` param — after the verify/fix loop
  settles (and only when not reverted), each named skill runs one READ-ONLY
  `CodePlan`-kind session over `git diff HEAD` (capped 60k chars) wearing the
  skill overlay; findings append to the report under "## Review — <name>".
  Untracked new files aren't in the diff (noted in code).
- Files >500-line lint respected by splitting: `regent-tools`
  `application/truncation.rs`, `regent-agent` `domain/collapse.rs`,
  permission acceptance test moved to `regent-tools/tests/permissions.rs`.
- Tests: full battery green across the six crates (383 tests incl. new
  fix_retry ×3, doom_loop, budget wrap-up ×3, truncation spill, permissions,
  todo, collapse ×2, explore ×1, code_skill ×2); `cargo fmt` clean; clippy
  clean for new code; `regent-gateway` compiles; CLI `tsc --noEmit` clean.
  Still owed: live `regent code` dogfood run (needs a running deacon).

## 2026-07-13 — regent-code v2 Wave 1 shipped: safe tool dispatch, edit-time diagnostics, bundled skills, coding system prompt

**Goal:** implement all four P0 items of [docs/plans/regent-code-v2.md](plans/regent-code-v2.md)
(Wave 1 — feedback and voice), leaving `regent code` strictly better.

- **1a — safe tool dispatch (gap L3, the latent correctness bug)**: the turn
  loop dispatched every tool call of a batch in parallel (`join_all`), so two
  `file_edit`s on the same file — or an edit racing the build in `terminal` —
  could interleave. Batches now partition into contiguous runs: read-only runs
  keep `join_all`, mutating runs execute serially in call order; results still
  re-attach in original call order. One deliberate deviation from the plan:
  instead of a `read_only` field on `ToolDefinition` (which would have touched
  ~90 struct literals across 37 files), the classification is a central
  `is_read_only_tool(name)` list in `regent-kernel/src/contracts/tool.rs` —
  same "flipping a tool is a deliberate one-line review" property, 1-file diff.
  Unknown names (all MCP tools) default to mutating/serial. New
  `regent-agent/tests/dispatch_order.rs` proves reads overlap, edits never do.
- **1b — edit-time diagnostics (gap H5)**: new
  `regent-code/src/infra/diagnostics.rs` — after a successful `file_edit` /
  `write_file` / `apply_patch`, the cheap per-language check runs (`cargo
  check -q --message-format=short` / `tsc --noEmit` / `node --check` /
  `python -m py_compile`; 10s timeout, first 15 error lines) and its findings
  ride the SAME tool result as a `diagnostics` JSON field (kept inside the
  JSON so the well-formed-JSON invariant holds — the plan's `<diagnostics>`
  XML-ish block was adjusted accordingly, prompt wording updated to match).
  Diagnostics can never fail an edit: spawn errors and timeouts degrade to
  log-only. Wired via a new `ToolCatalog::wrap_executor` seam into BOTH
  `CodeHarness::execute_phase` and the deacon's live `code.start` sessions
  (the plan scoped it to CodeHarness only, but CodeHarness is test-only today
  — without the deacon wiring the feature would be dead in production).
  `create_session_keyed`'s `plan_mode: bool` became `SessionKind`
  (Chat/CodePlan/CodeExecute) to carry the distinction.
- **1c — bundled skills + harness skills (gaps S2/S3/R1/R2)**: three skills
  now ship in the binary via `include_str!` —
  `regent-skills/skills/{ponytail,code-reviewer,secure-code-guardian}/SKILL.md`
  (bodies per the plan companion §4). `SkillLibrary` merges them under the
  disk repository: disk wins on name collision (user override by name),
  bundled fill the gaps; the curator can't touch them (`created_by` guard +
  they're not on disk + pinned). `code.plan`/`code.start` accept an optional
  `skill` param — the deacon resolves the body via the library and appends
  it to the frozen session prompt as a tier-0 ledger segment (`## Active
  skill: <name>`); unknown names are a hard RPC error. `code_task` gained the
  same optional `skill` parameter (described so the model picks `ponytail`
  for minimal asks); `regent code` gained `--skill <name>`. `code_task`'s
  description was tightened to keep the resident catalog under the 2.2k-token
  SPL P4 gate.
- **1d — coding system prompt (gaps P1/P2)**: `regent-agent`'s 408-line
  `domain/prompts.rs` split into `domain/prompts/{mod,system,constitution,coding}.rs`
  (public API unchanged, constants moved verbatim). New `CODING_PROMPT`
  (communication · tool discipline · verification · scope, per companion §5)
  is prepended to the surface prompt for both harness phases by
  `CodeHarness::new`; `plan_prompt`/`execute_prompt` lost the guidance the
  overlay now owns. `SYSTEM_PROMPT` gained the two ports: memory-application
  conventions (apply without narrating, relevant only, sensitive stays
  unprompted, update-don't-duplicate) and the own-mistakes-plainly line.
- Tests: 359 green across kernel/agent/tools/skills/code/deacon, including
  new `dispatch_order.rs`, `regent-code/tests/diagnostics.rs`,
  `deacon_basics/code_skill.rs`, bundled/override/curator-guard skills tests.
  `cargo fmt --check` clean on all six crates; clippy clean for the new code
  (pre-existing warnings elsewhere left alone); `regent-gateway` compiles;
  CLI `tsc --noEmit` clean. Wave 1 acceptance not yet dogfooded with a live
  `regent code` run (needs a running deacon + model credit).

## 2026-07-13 — desktop: map geocode fixes — right country, no more chat-triggered pop-ups, street map hand-off keyed on the real location

**Goal:** three user reports — "where's the Tesla factory in China" landed
on the US; a normal chat sentence popped the map unprompted; the street
map still didn't appear after the globe zoom, for a third time.

- **Wrong country**: `WHERE_SUBJECT_RE`'s non-greedy subject capture, when
  the utterance repeats "is" right after "where" (STT's "where IS tesla
  factory is on china"), swallowed that first "is" into the subject
  ("is tesla factory" instead of "tesla factory"). Separately, the
  subject+place combo was joined with a bare space ("tesla factory
  china"), which Nominatim reads as three loose keywords — letting the
  Tesla the COMPANY (US-headquartered) outrank the Shanghai gigafactory.
  Fixed both: strip a leading is/are/was/were the same way a leading
  article is already stripped, and join subject+place with a comma
  ("tesla factory, china"), which Nominatim reads as containment.
- **Chat popping the map unprompted**: the location cues (`where's`,
  `capital of`, `flights? to`, …) are necessarily loose — they can fire on
  ordinary sentences ("the flight to Denver was late", "where's the bug"),
  and Nominatim's database is large enough that almost any word matches
  SOME obscure hamlet or stream on Earth. Added an `importance` floor
  (Nominatim's own 0–1 significance score) of 0.2 — small-town-and-up
  passes, trivial word-matches don't. `geocodePlace` now asks for 3
  candidates and picks the first past the floor.
- **Street map still not appearing**: found the REAL mechanism this time.
  The "already flown" gate keyed on the raw `places` candidate-string
  list, but that list can legitimately differ between the two resolves of
  the same utterance (setHeard's early resolve vs. the turn-end resolve) —
  a transient Nominatim hiccup on one of two near-simultaneous lookups
  isn't cached, so it silently drops or re-adds a query variant. A
  different-shaped-but-same-place list read as "new place", resetting
  `detail` and re-flying right as the street map was about to show — which
  is exactly "the globe zooms in but nothing else ever appears". The gate
  now keys on where `hits[0]` actually landed (rounded lat/lon), decided
  AFTER resolving, not on the pre-resolve candidate list. `MapBackdrop.tsx`.
- `geocode.test.ts` updated + a new test for the doubled-"is" STT case;
  Desktop `tsc --noEmit` clean; `bun test features/butler/` — 28 pass.

## 2026-07-13 — desktop: camera capture construction can no longer strand a Butler call on "Listening"

**Goal:** the user's report — Butler Mode gets stuck on "Listening" and
never responds to speech.

- `startCameraFrames` runs BEFORE `useButlerCall` flips the phase to
  'listening' (mic setup, then camera frames, then the audio graph, then
  listening). The just-added `new ImageCapture(track)` there was
  unguarded — if it throws (unsupported API surface, or the video track
  not yet in a grabbable state on some camera/driver), the whole async
  setup aborts and the mic/VAD loop never gets wired up at all, silently.
  Wrapped in try/catch: a construction failure now just means no camera
  frames this call (matches the existing no-video-track no-op) instead of
  losing the call.
- Not fully confirmed as THE root cause — the running voice server went
  down mid-investigation before a live repro completed (`/health` showed
  `warm:true, asr:true, tts:true` moments earlier, then the port stopped
  answering). If "stuck on Listening" recurs after this, open DevTools in
  the Butler window (F12) and check the console for an error the moment it
  happens — that pinpoints it definitively.
- Desktop `tsc --noEmit` clean.

## 2026-07-13 — desktop: camera capture no longer stalls the call's audio thread; map bbox hardened against bad geocode data

**Goal:** the user's report — ASR/listening felt massively slower after
today's earlier improvements — plus a second look at the map fly-in
("still doesn't show").

- Root cause of the ASR slowdown: `startCameraFrames` (added earlier today
  for the Butler camera fix) drew the LIVE `<video>` element to a canvas
  every 2.5s via `drawImage()` — a synchronous GPU→CPU readback that stalls
  the main thread for tens of ms. The call's VAD loop
  (`ScriptProcessorNode.onaudioprocess`, deprecated but still main-thread)
  runs on that SAME thread, so each camera tick could delay audio frames —
  felt as random sluggishness right after the camera feature shipped.
  Switched to `ImageCapture.grabFrame()`, the browser API built for this —
  it decodes off that hot path — and dropped the now-unneeded hidden
  `<video>` element entirely. Also guards against overlapping grabs if a
  tick is still encoding when the next fires. `cameraFrames.ts`.
- Map bbox hardening: `validBbox()` (new, `geocode.ts`) rejects a
  degenerate Nominatim box (NaN, or south≥north/west≥east) that would
  otherwise hand MapLibre invalid bounds and render nothing — both
  `MapBackdrop`'s altitude calc and `StreetMap`'s fit now fall back to the
  old point+zoom instead of silently failing. Re-verified the hand-off-ref
  fix from the prior patch is still correct on a fresh read; if the map
  still doesn't appear after this, it needs a console/repro from the user
  to isolate further (tile CSP, a specific query's geocode result, etc).
- Desktop `tsc --noEmit` clean; `bun test` on geocode/presentation green.

## 2026-07-13 — desktop: street map actually appears after the globe fly; engine warm-up shows a loader, not a red error

**Goal:** two user reports — after the globe zooms in, the street map never
appears ("only the globe, zoomed in"), and "ASR: loading local engines…"
showed as a red error while the voice server warmed up.

- Street-map hand-off race: the globe→street-map hand-off timer was owned
  by the places-effect's cleanup. The turn-end re-raise of the SAME places
  re-ran the effect — cleanup cleared the timer, the re-run early-returned
  on the flown-key match and never rescheduled — so `detail` never set and
  the StreetMap never mounted. The timer now lives in a ref cleared only by
  a DIFFERENT place or unmount (`MapBackdrop.tsx`).
- Warm-up UX: the voice server reports engine load/download progress
  through a turn's `error` line; the client painted it red. New
  `isWarmingError()` (phase.ts) classifies loading/downloading/warming
  lines; ButlerView shows a Loader + "Waking the voice engines…" instead,
  in both the connecting and in-call spots. Real errors stay red.
- Desktop `tsc --noEmit` clean.

## 2026-07-13 — desktop: Butler map fits the place asked for; diagram side-image actually finds images

**Goal:** two user reports — the map "doesn't zoom enough to a specific
place", and the floating (supplementary image) window beside diagrams
stopped appearing.

- Geocode now carries Nominatim's `boundingbox` on each hit. The globe's
  landing altitude scales with the place's size (country ≈ high vantage,
  landmark ≈ close 0.18 swoop — was one fixed 0.5 for everything), and the
  street map opens FITTED to the bbox then does a cinematic settle-in
  (padding 96 → 48, POIs sink to zoom 17; was a fixed zoom-15 block at the
  centroid of whatever was asked). Reduced motion starts at the final
  frame. `geocode.ts`, `MapBackdrop.tsx`, `StreetMap.tsx`.
- The diagram's side image looked up Wikipedia by the DIAGRAM TITLE as an
  exact page name — wordy titles ("How Fuel Injection Works") 404'd, so no
  window opened. `topicImage.ts` now falls back to Wikipedia opensearch for
  the nearest article, then fetches that summary. Still best-effort/silent.
- Desktop `tsc --noEmit` clean.

## 2026-07-13 — agent: Butler obeys explicit "search for…" again (visual-first prompt override)

**Goal:** the user's bug — on a Butler call, asking Regent to search for
something never used web_search/browser/computer_use; it answered from
memory instead.

- Cause: `VISUAL_EXPLAINER`'s MAP/PICTURE-BEFORE-TOOLS rules (added to stop
  tools hijacking the globe/diagram) ended with an unconditional "never
  open the web as the first move" — the model read it as a tool ban even
  when searching WAS the task. The tools themselves were fine
  (`REGENT_COMPUTER_USE=1` defaults on for the Butler deacon).
- Added an EXPLICIT-ASK-OVERRIDES clause: visual-first governs only how
  Regent chooses to answer from its own knowledge; a direct "search/look
  up/google/open/click/find online" instruction runs the matching tool
  immediately (pure where-is-a-place asks still belong to the live map).
- `prompts.rs` only; agent prompt tests 6 ✓; release deacon rebuilt; voice
  server stopped so the next Butler call speaks with the new prompt.

## 2026-07-13 — voice: Kokoro voice applies LIVE + a speed slider (desktop, deacon, voice server, CLI)

**Goal:** the user's bug — picking a voice in Settings didn't take effect
until a voice-server restart — plus a requested speech-speed slider.

- `regent-voice-server` now re-reads `REGENT_KOKORO_SPEAKER` and the new
  `REGENT_KOKORO_SPEED` from `$REGENT_HOME/.env` on EVERY synthesis
  (sherpa's `create()` takes speaker + speed per call), so a settings
  change speaks on the very next reply — no restart. File wins over the
  spawn-time process env; speed clamps to 0.5–2.0 (`infra/sherpa.rs`).
- `voice.set` gains `kokoro_speed` (validated 0.5–2.0) → `.env`;
  `voice.status` reports `kokoro_speed` (default "1"). The reply note now
  says "picks this up on its next reply" when only kokoro keys changed.
- Desktop Settings → Voice: new "Voice speed (local call)" slider
  (0.5×–2×, commits on release); voice/speed hints now say "next reply".
- CLI: `regent voice status` shows the local call voice by name + speed.
- Verified: deacon lib tests 92 ✓, voice-server `--all-features` check ✓,
  Desktop + CLI `tsc --noEmit` ✓; release deacon + voice server rebuilt.

## 2026-07-13 — desktop: Butler globe actually flies to the asked-for place

**Goal:** the user's bug — asking "show me where Manila is" in Butler Mode
raised the globe but it stayed on the default world view instead of flying
to the place.

- `MapBackdrop` marked a place set as "already flown" (`flownKeyRef`)
  BEFORE doing any work. Under React StrictMode the first effect pass is
  cancelled (its globe destroyed) but the ref survives, so the second pass
  saw the key latched and returned without ever flying. Same latch also
  permanently blocked a retry after a transient geocode miss.
- The key is now marked only at the moment the fly starts — cancelled or
  empty-hit runs leave it unlatched so the next raise flies. One-line move
  in `features/butler/presentation/MapBackdrop.tsx`; `tsc --noEmit` clean.

## 2026-07-13 — deacon/tools: gateway screenshots land in the real ~/.regent/artifacts, never a repo-local `.regent`

**Goal:** the user's bug — asking Regent over a gateway platform for a
screenshot created a `.regent/` folder inside the repo instead of using
`$REGENT_HOME/artifacts`.

- Two causes: (1) `artifacts_line()` (deacon) and the gateway bin only
  emitted the artifacts directive when `REGENT_HOME` was set — unset env ⇒
  no directive ⇒ the agent invented a cwd-relative `.regent/`; (2) external
  (gateway/webhook) sessions are jailed to the deacon's cwd, so the real
  `~/.regent/artifacts` was unwritable even when named.
- The directive now always resolves the ABSOLUTE home (env else
  `~/.regent`), covers screenshots/files-to-send explicitly, and says never
  to create these elsewhere.
- `ToolContext` sandbox is now multi-root: `allow_subtree()` widens a jail
  by exactly one subtree. External sessions get `$REGENT_HOME/artifacts`
  (created on demand) — `.env`/state.db at the home ROOT stay sealed
  (tested: `allow_subtree_widens_the_jail_to_exactly_that_subtree`).

## 2026-07-13 — deacon: model.list stops offering Claude models nobody configured

**Goal:** the user's bug — the composer model menu listed 4 Claude models
as picks although no Anthropic provider exists in config.yaml; picking one
failed at call time.

- The static Claude catalog is now offered only when config has an
  anthropic-kind provider, or on legacy no-config boots (whose default
  provider IS Anthropic via REGENT_API_KEY). Configured providers' models
  (`<provider>/<model>` rows) are unaffected — those were always live.
- Tests updated + `dispatcher_model_list_offers_claude_menu_with_anthropic_provider`.

## 2026-07-13 — deacon/desktop: pick Regent's local call voice (Kokoro speaker)

**Goal:** the user's ask — a settings control for the speaking voice,
listing all available voices. The local call path (Kokoro) had NO knob
anywhere; only the `REGENT_KOKORO_SPEAKER` env var, undocumented in UI.

- `voice.set` gains `kokoro_speaker` (voices-file index, validated int) →
  `REGENT_KOKORO_SPEAKER` in `$REGENT_HOME/.env` — the exact
  `whisper_size` pattern; both voice-server spawners (desktop `voice.rs`,
  CLI `voiceServe.ts`) already merge `.env`, so no spawner changes.
  `voice.status` reports the effective value (default "0").
- Settings → Voice gains "Voice (local call)": the 11 kokoro-en-v0_19
  speakers by name (af/af_bella/…/bm_lewis), applied on next voice-server
  restart (the existing `note` says so).

## 2026-07-13 — agent: background reviews batch instead of flooding (800 sessions/2wk fix)

**Goal:** the review-session flood from the 2026-07-13 handoff — 800
`source='review'` sessions / 30.4M input tokens in 14 days, more than all
real chats combined.

- Root cause: `run_turn` forked a review after EVERY turn, and each review
  replayed the WHOLE transcript from message 0 — O(n²) token burn over a
  conversation's life.
- `ReviewSetup` gains `min_new_messages` (deacon/repl/gateway set 8): a
  review now spawns only once that many unreviewed messages accumulate,
  and its snapshot contains ONLY the unreviewed slice (`Agent.reviewed_len`
  mark; resume starts the mark at the restored length so old history is
  never re-reviewed). Expected: ~4× fewer review sessions and per-message
  cost that no longer grows with conversation length.
- Known ceiling (deliberate): a sub-threshold tail at session end is never
  reviewed; add a shutdown flush only if that loss matters.
- Files: `regent-agent` `application/review.rs`, `application/agent/mod.rs`,
  `bin/repl.rs`; `regent-deacon` `session_manager/build.rs`;
  `regent-gateway` `bin/gateway.rs`; test `tests/learning_loop.rs` (new
  `reviews_batch_and_replay_only_unreviewed_messages`).

## 2026-07-13 — desktop: Butler shares the camera with the agent

**Goal:** the user's bug — "Butler mode can't access the camera": asking
"what am I holding?" in Butler found no live frame (the `camera_capture`
tool's ffmpeg fallback then fails without ffmpeg / with the camera busy).

- The backend was already complete (voice server `/call/frame` →
  `$REGENT_HOME/voice/camera-frame.jpg` → `camera_capture`); Butler simply
  never posted frames — its `getUserMedia` was audio-only, unlike the web
  call page.
- Butler now requests mic+camera together and falls back to mic-only if the
  camera is denied/absent (the call never dies over the camera). New
  `features/butler/data/cameraFrames.ts` (port of
  `regent-web/hooks/localCall.ts` `startCameraFrames`) posts a small JPEG
  every 2.5s while the call runs; wired in `useButlerCall.ts`.
- Settings → Voice gains a **Camera** picker (mirror of the mic picker):
  choose which camera Butler shares, persisted to localStorage
  (`shared/infrastructure/camera.ts`, `CameraPicker.tsx`, `VoiceSection.tsx`);
  Butler pins the pick via `cameraConstraint()`. An unplugged saved camera
  just falls back to mic-only — same catch as denial.

## 2026-07-11 — deacon: GPT-5.6 family in the provider catalogs

- OpenAI's GPT-5.6 family (GA 2026-07-09) joins the curated defaults:
  `gpt-5.6-sol` (flagship; the bare `gpt-5.6` alias routes to it),
  `gpt-5.6-terra` (balanced), `gpt-5.6-luna` (cost-optimized) — native
  `openai` kind and the matching `openai/…` OpenRouter slugs.

## 2026-07-11 — deacon: SPL P5 — the Distiller, human-gated persona consolidation

**Goal:** phase P5 of the token-efficiency proposal (ADR-035): budgets
fail-closed at the write; the Distiller keeps writers from ever hitting
that wall — without ever letting a background model call rewrite the
agent's identity unreviewed.

- `application/distiller.rs` — a 6-hourly watcher checks every budgeted
  persona row (constitution/soul/about + facets); past 80% fill it runs ONE
  consolidation model call (merge duplicates, compress, lose nothing
  semantic) and stages the result as a pending write (`persona_rewrite`,
  stable id `distill:<key>` = at most one live proposal per store, 7-day
  TTL then auto-rejected by the existing expiry loop).
- **Always human-gated, for every store including soul and constitution**:
  the proposal sits in the same `memory.pending` queue the approval UI
  reads; nothing lands without `memory.approve`.
- Approval applies through the BUDGETED `set_persona` path, after backing
  the old content up to a non-rendering `backup.<key>` persona row (DB —
  personas never live in plaintext files; unbudgeted because pre-rewrite
  content is exactly what can exceed the budget). Reject/expiry leave the
  store byte-identical.

## 2026-07-11 — deacon/desktop: code runs no longer freeze the app; Stop/approvals bind to the run session

**Goal:** the user's bug — "Stop generating doesn't work, in code and even
on the chat page" once a code run had started.

- `code.plan`/`code.start` (and `mom.run`, `providers.test`) were awaited
  INLINE in the serial stdio dispatcher loop, so a minutes-long run queued
  every other request behind it — `turn.interrupt`, chat turns, settings:
  the whole app froze until it finished. All four now run detached; the
  response still carries the original request id.
- `code.start` executes in a NEW session the client only learned about when
  the run ended — Stop, approval responses, and event streaming all
  targeted the read-only plan session. The deacon now announces
  `code.started {session_id}` before the execute turn; the desktop rebinds
  its stop/approval/event handlers to it.
- Also: applying a primary on the Model page now re-points the ACTIVE model
  (chat previously kept routing through the old model and silently demoted
  the new primary to a fallback), `set_model` emits `model.changed` so the
  composer pill and status bar update live in both directions, and boot
  prefers `agents_defaults.primary` over the legacy `model.default` so the
  applied pick survives restarts.

## 2026-07-11 — deacon/skills/tools: SPL P4 — adaptive tool tiering, skills MRU cap, Tier-1 ceiling, context.budget

**Goal:** phase P4 of the token-efficiency proposal (ADR-035): make catalog
and session-tier growth pay-when-used instead of a per-turn tax.

- **Adaptive tool tiering** (§3.5): at session build, tools with no recorded
  use in the last 30 days are auto-deferred — the messages ledger IS the
  counter (`Store::tool_use_counts`), no new write path. `tools.pinned`
  (core file ops, terminal, web_search, memory_search) never defers;
  `tools.auto_tier: false` turns it off; a store read error fails open to
  the full catalog. Deferred tools stay directly callable and loadable via
  `load_tools` (whose per-tool hook shrank 80→60 chars — with most tools
  deferred it dominates the loader's schema). Acceptance: the default
  model-facing catalog now fits ≤1.5k tokens (CI-gated); the ≥90%
  unprompted load-and-use eval runs post-ship, as with P2's cache-hit rate.
- **Skills index MRU cap** (§3.4): past 24 skills the index renders only
  the most-recently-used lines (usage sidecar ranks; kept set re-sorted by
  name for byte-stable rebuilds) plus "…and K more — skills_list shows all."
- **Tier-1 ceiling** (§3.4, the ECC read-side cap): the SESSION tier injects
  at most 36k chars even when every store is at its own budget; trimming
  walks from the end (memory before skills before persona — both trimmed
  blocks stay retrievable via memory_search/skills_list) with a marker.
- **`context.budget` op** (§3.4): per-Ledger-segment chars + est. tokens,
  tier totals, and the serialized tool-definitions size for an open session
  — prompt-size audits become a one-call measurement.

## 2026-07-11 — deacon/desktop: a custom model applied on the Model page now joins its provider's catalog

**Goal:** the user typed a custom model (GLM 5.2) via the Model page's
"Custom…" entry and applied it — it never appeared in the "choose a model"
dropdown or `regent model list`, because Apply wrote only
`agents_defaults.primary` and every catalog reads `providers.<name>.models`.

- `config.set` on an `agents_defaults.*` path now adopts any primary/fallback
  model that no catalog offers (neither the provider's configured `models:`
  nor its kind's curated defaults) into `providers.<name>.models`, through
  the same validated write gate. Curated ids are still never written back;
  unknown providers are skipped; adoption failure keeps the original write.
- `ProviderSpec::curated_defaults()`/`offers()` extracted (the ollama.com
  special case now lives once, shared with `providers.models`).
- Desktop: `useMainModels` refetches config + catalogs after a successful
  write, so the adopted model shows in the dropdown right away.

## 2026-07-11 — providers/deacon: SPL P2 — Anthropic cache_control adapter, cadence-gated

**Goal:** phase P2 of the token-efficiency proposal (ADR-035): explicit
prompt-cache breakpoints where the cadence study says they pay, cache usage
surfaced end to end, and full-price turns attributed to their cause.

- `regent-providers` — `ChatRequest.cache: Option<CachePolicy>` (fail-open:
  `None` = today's request). The Anthropic adapter places up to 3 breakpoints
  when a policy is on: last tool def (caches the tool block), the system
  block, and the last history message before the current user turn. 1h TTL
  emits `{"ttl":"1h"}`. No policy → no breakpoints anywhere (review/delegate
  stop paying the 1.25× write they could never read back).
- `TokenUsage` gains additive `cache_read_tokens`/`cache_write_tokens`;
  mapped from Anthropic (`cache_read_input_tokens`/`cache_creation_…`, both
  sync and streaming) and OpenAI-compatible implicit caching
  (`prompt_tokens_details.cached_tokens`).
- `regent-deacon` — `domain/cache_policy.rs` encodes the cadence-study
  verdicts per session source: `deacon`/`daemon` → 5m, `telegram` → 1h,
  `review`/`delegate`/unknown → none. Resolved once at session build into
  `AgentConfig.cache_policy`.
- `turn.complete` gains additive `cache_read_tokens`/`cache_write_tokens`
  and `cache_reset` (`routing` > `compaction` > `failover` > `pruning`,
  highest-priority cause wins; omitted on a clean turn). A routing-epoch
  provider swap stamps the next turn `routing`; a mid-turn fallback stamps
  `failover`; compaction and P3 pruning stamp themselves.
- CI: breakpoint placement on/off, TTL field, first-turn shape, usage
  mapping, and a 10-turn warm-cache session asserting ≥70% cache_read
  passthrough on turns ≥2.

## 2026-07-11 — store/deacon: boot sweep deletes abandoned empty sessions

**Goal:** bug #3 — "Don't save Empty new sessions." The desktop already
creates sessions lazily; the rail clutter was 200+ pre-existing abandoned
rows (no messages, no turns) that nothing ever cleaned up.

- `Store::delete_empty_sessions(min_age_secs)` — deletes sessions with no
  messages, no turns, and no child sessions (a delegation parent must
  survive for its child's FK), older than the grace period.
- Deacon boot runs the sweep with a 1h grace so a session another live
  process just created is never swept out from under it.

## 2026-07-10 — deacon: SPL P1 — the Stable-Prefix Ledger, tier-hash telemetry, cadence study

**Goal:** phase P1 of the token-efficiency proposal (ADR-035): make the
prompt's byte-stability measured and enforced instead of accidental, and
answer whether explicit provider caching pays before P2 builds it.

- `domain/ledger.rs` — the Ledger: the one code path that concatenates the
  system prompt, each segment tier-classified (Tier 0 PROCESS / Tier 1
  SESSION). Render is byte-identical to the old `format!` (a test proves it).
  Build-time baseline = rendered prompt + sealed tool-defs serialization
  (sealed AFTER disable/defer/plan-restrict, so it matches the wire).
- `session_manager/telemetry.rs` — per-turn fail-open check of what the agent
  actually sends (frozen prompt + re-serialized defs; never live store reads,
  so mid-session persona edits don't false-alarm). A mismatch logs a
  `cache_bust` warning naming tier + segment; `turn.complete` gains additive
  `tier0_hash`/`tier1_hash` fields. Resume rebases onto the stored prompt
  (stored-prompt-wins) so legacy sessions never false-bust.
- CI gates (`tests/deacon_basics/ledger.rs`, 7 tests): 80k-char fixed-prefix
  ceiling; Tier 0/1 hash stability across a 50-turn synthetic session + 50
  defs re-serializations; an injected timestamp, a mutated persona span,
  reordered defs serialization, and a trailing injection each trip the check
  with correct attribution.
- **Cadence study** (`docs/audits/2026-07-10-cadence-study.md`, from 1,047
  sessions / 1,523 turns): explicit caching pays on `deacon`/`daemon` (5m TTL,
  expected reads ~0.96) and `telegram` (1h TTL, 1.00); `review` is a hard
  no-breakpoints surface (660/660 sessions single-turn). P2 codes against
  these per-surface verdicts.
- Also: the two `dispatcher_backfill_titles_*` tests were stale since
  528629b made the sweep detached (`{started}` reply + `session.titled`
  events); they now assert the report on `SessionManager::backfill_titles`
  directly and the RPC's ack shape.

## 2026-07-10 — deacon: saving a provider key now surfaces the provider in Settings → Model

**Goal:** the user added an NVIDIA key on the API Keys page and the provider
never appeared on the Model page — not a refresh bug: the Model picker lists
only `config.providers`, and saving a key never created an entry there.

- `env.set` — when the saved var is the conventional key of a known
  `ProviderKind` (e.g. `NVIDIA_API_KEY` → `nvidia`) and config has no provider
  of that kind, nor any entry reading that var, nor an entry under that name,
  a minimal `providers.<kind>` entry (`kind` + `api_key_env`) is auto-added
  THROUGH the validated `config.set` write gate (whole-file revalidation; a
  taken name is never clobbered). Best-effort: the key save already succeeded,
  a failed auto-add only warns. The reply note says the provider was added.
  Curated default models then flow from `providers.models` — the Model page
  offers the provider on its next open. Numbered slots (`_2`+) count as their
  base; `REGENT_API_KEY`/non-provider keys (Tavily, Slack…) add nothing.
- `config_ops::set_config_path` → `pub(super)` so env_ops reuses the one
  validated config-write path instead of growing a second one.
- Tests: the generated entry survives the real write gate end-to-end;
  dedup/never-clobber matrix (same kind under another name, var claimed by
  another entry, taken name, generic/lookalike keys).

## 2026-07-10 — fix: constitution enable-path test broken by the persona budgets (P0)

**Goal:** `constitution::tests::enable_upgrades_a_full_document_row_but_not_a_user_edit`
failed since the persona budgets shipped: it recreates a pre-vectorization
legacy row (the FULL ~13.8k-char document) through `set_persona`, which the
new 12k constitution budget now rejects. Legacy state predates the budget —
recreating it must not go through a gate that postdates it.

- `regent-store` — `set_persona_unbudgeted`: the raw upsert split out of
  `set_persona` (which keeps the budget check for every tool/RPC/CLI path);
  re-exported `persona_budget` from the crate root.
- The test seeds the legacy row via the unbudgeted path; assertions unchanged.
- New production-invariant guard in `enable_seeds_core_row…`: the CORE the
  enable flow writes must fit `persona_budget("constitution")` — if the
  shipped core ever outgrows the budget, enabling breaks at runtime, and CI
  now says so instead of production.

## 2026-07-10 — proposal v2: Stable-Prefix Ledger revised after adversarial review

**Goal:** v1 was reviewed for holes the same day; eight were found and the
plan revised in place (§9 of the doc lists all eight and where each is fixed).

- Honest expected-outcomes table up front: −85 to −93% billed input on
  caching providers in the best case, only the raw −50% on non-caching
  providers, 0% accuracy change for P0–P2 by construction; full-price turns
  (compaction, failover, routing) are counted, not ignored.
- Turn-cadence study is now a P1 prerequisite — a cache *write* costs 1.25×,
  so sparse-turn sessions can make explicit caching cost MORE than nothing;
  the Anthropic adapter is cadence-gated (no breakpoints when expected
  reads < 1).
- Live persona edits reconciled with the frozen SESSION tier: mid-session
  changes ride as a Tier-3 delta and fold in at the next session build —
  immediate effect, zero cache busts.
- New §3.8 + P3: **tool-result pruning** — stub out stale tool outputs from
  history (re-fetchable by design), the biggest history-side lever and v1's
  largest omission.
- Distiller is now human-gated for EVERY store including soul/constitution
  (was an ungated identity-drift channel); tool-deferral acceptance became a
  failure-mode eval (≥90% unprompted load-and-use); P2 acceptance raised
  from `cache_read > 0` to ≥70% cache-read on turns ≥2.
- ECC downgraded from "validation" to mechanism prior art — a source-level
  scan found it publishes no benchmarks.

## 2026-07-10 — proposal: the Stable-Prefix Ledger (token-efficiency architecture)

**Goal:** a researched, long-term plan for token efficiency and reliability,
building on the same-day audit. Written to
`docs/proposal/token-efficiency-architecture-v1.md`.

- Core invention: treat the prompt as a **ledger** — an ordered, byte-stable
  contract in four stability tiers (process / session / turn / volatile) with
  per-tier hash telemetry that catches cache-busting regressions on the first
  affected turn, plus a CI prefix-size ceiling.
- Per-provider cache adapters in regent-providers: explicit `cache_control`
  breakpoints for Anthropic (reads ~0.1×), byte-stability alone for
  OpenAI/DeepSeek/Gemini implicit caching, detection mode elsewhere;
  fail-open by design. Cache usage passthrough → desktop cached/fresh meter.
- Governance: read-side injection ceilings and confidence-weighted injection
  (ECC-inspired), usage-earned tool residency over ADR-031's deferral seam,
  skills-index MRU cap, `context.budget` op, and a proactive Distiller that
  consolidates budgeted stores at 80% fill through the memory gate.
- Grounded in Anthropic's context-engineering research (compaction,
  note-taking, JIT retrieval, sub-agent isolation — all already present in
  Regent in embryo) and the measured provider-caching market. Roadmap P0–P4
  with acceptance criteria; expected steady state ~2–4k billed tokens/turn on
  caching providers with zero prompt-content changes.

## 2026-07-10 — deacon: token audit — persona budgets end the 30k-input turns

**Goal:** every chat turn was burning ≥30k input tokens; find the cause and
fix it without touching the system prompt's design.

- Measured breakdown (live probes against the real store): `soul` persona row
  47,647 chars (~11.9k tokens), `about` 15,291 (~3.8k), constitution (opt-in)
  9,056 (~2.3k), static SYSTEM_PROMPT+CAPABILITIES ~4.3k tok, 30 tool schemas
  ~3.3k tok, graph memory block ~0.9k tok (already budgeted: 2,200+1,375
  chars). Persona alone was over half the spend.
- Root cause: graph memory had hard char budgets from day one, but persona
  rows had NONE — `update_persona`'s `append` action let the agent accrete
  episodic call-notes into `soul`/`about` forever (98 bullets, many repeated
  verbatim 2-3×), and the whole block rides every turn's system prompt.
- `regent-store` — `persona_budget(key)`: soul 8,000 chars, about 6,000,
  about facets 2,000, constitution 12,000 (deliberate opt-in layer). Enforced in
  `Store::set_persona` (covers the tool, RPC, and CLI); over-budget writes
  fail with guidance to consolidate — the same pattern as graph entries.
  New `StoreError::PersonaBudget` + tests.
- One-time consolidation of this machine's oversized rows: originals backed
  up to `~/.regent/persona-backup-2026-07-10/`, then `soul` and `about`
  rewritten distilled (every distinct durable rule kept, duplicates merged,
  stale facts dropped) — ~5.9k + ~3.5k chars. Expected input-token drop:
  ~12k/turn (30k → ~18k).
- Startup audit (also asked): deacon cold boot 34ms, session.list(1000) 29ms,
  session.create 6ms, memory.list 2ms — the backend was never the
  bottleneck. The afternoon's slow loads were the serialized dispatcher
  wedged behind the blocking title sweep (fixed earlier today); remaining
  startup cost is WebView2 window creation + the 300ms splash fade.

## 2026-07-10 — desktop: Butler audio off the phone-call path

**Goal:** Butler's TTS sounded muffled/"noise cancelled" and music playing in
other apps got attenuated whenever Butler Mode was open — even with the
machine's `UserDuckingPreference = 3` fix (2026-07-09) still in place.

- Root cause: one `AudioContext` hosted BOTH the echo-cancelled mic capture
  and Regent's reply playback. Windows opens that context as a
  communications session, and audio drivers (Intel Smart Sound here) apply
  their voice-call DSP to the render path — narrow-band TTS and system-wide
  music muffling that the OS-level ducking preference doesn't control.
- `useButlerCall.ts` + `callLoop.ts` — replies now render through a SEPARATE
  capture-free `AudioContext` (plain media path, full quality). The playback
  context carries its own analyser; `analyserRef` swaps per phase so the
  voice dots follow your voice while listening and Regent's while speaking.
  Chromium's AEC references all process output, so barge-in keeps working.
- `shared/infrastructure/mic.ts` — capture keeps `echoCancellation` (barge-in
  depends on it) but drops `noiseSuppression`/`autoGainControl`: the voice
  server's own VAD/robustness layer already handles noise, and the WebRTC
  processing made captured speech sound processed.
- Session names ("still deacon.[id]"): no new code — the convo predated the
  detached-backfill deacon; the rebuilt binary titles first turns live and
  sweeps old sessions at boot. Needs the app restart.

Verified: `bun run build` (tsc + vite) green.

## 2026-07-10 — deacon+desktop: NVIDIA NIM provider, voice model dropdowns, fallback dedupe

**Goal:** follow-ups from live testing: the voice model pickers still showed a
text field for the `local` provider, NVIDIA (build.nvidia.com) wasn't a
provider option, and "+ Add fallback" generated repetitive chains.

- `regent-deacon` — new `ProviderKind::Nvidia` (NVIDIA NIM,
  `https://integrate.api.nvidia.com` OpenAI-compatible, `NVIDIA_API_KEY`):
  one variant + one line per match (provider_kind.rs), a curated NIM catalog
  (provider_catalog.rs, org-prefixed ids), and an API Keys row
  (env_ops.rs `LLM_KEYS`). Configure a provider with `kind: nvidia` and the
  models page offers the catalog.
- `features/settings/presentation/VoiceSection.tsx` — the ASR/TTS model
  picker is now ALWAYS a dropdown (configured value + curated options +
  Custom… free-text escape); `local` lists the SpeechConfig default weights
  (qwen3-asr/tts-1.7b), so the default setup no longer lands on a bare text
  field.
- `MainModelsSection.tsx` + `useMainModels.ts` — "+ Add fallback" walked key
  SLOTS before models, producing glm·Key1 / glm·Key2 / next-glm·Key1 / …
  chains; it now picks distinct (provider, model) pairs first and only rides
  a second key once every catalog model is in the chain. Fallbacks equal to
  the primary or exact earlier repeats are also dropped at load, so a config
  written before dedupe shipped displays (and re-persists) clean.

Verified: `cargo test -p regent-deacon --lib` 81/81; `bun run build` (tsc +
vite) green; deacon rebuilt 17:19 via the rename-aside pattern — restart the
app to pick it up.

## 2026-07-10 — desktop: bug batch — sessions store, catalog picker, scroll pin

**Goal:** fix seven reported desktop paper-cuts: the scroll-to-bottom arrow,
the model catalog not pickable, static "deacon · id" session names, slow
loading, stale sessions after Butler, dead session-menu actions, and the
Ollama local/cloud split.

- `features/shell/viewmodels/useSessions.ts` — rewritten onto ONE module-level
  store (`shared/state/store` seam): the rail, titlebar menu, messaging, and
  Archived settings previously each fetched 1000 rows and held diverging
  copies, so a rename/pin/delete from the title menu "did nothing" anywhere
  else and boot paid 4× the fetch. Mutations now reflect everywhere instantly;
  exported `refreshSessions()`; plus a one-shot `session.backfill_titles`
  sweep after first load so pre-titling sessions stop showing "deacon · 3f9c2a"
  (new sessions were already titled live via `session.titled`).
- `app/presentation/AppShell.tsx` — Butler close now calls `refreshSessions()`:
  its voice calls land sessions through the voice server's own deacon, so no
  notification ever reaches the webview.
- `features/chat/presentation/ChatView.tsx` + `shared/ui/ScrollToBottomButton.tsx`
  — the arrow was an abspos child *inside* the scroll container, so it scrolled
  away with content; moved out to the pinned wrapper (offset above the
  composer via new `className` prop).
- `features/settings/viewmodels/useMainModels.ts` — the picker dropped the
  deacon's `providers.models` catalog whenever the provider had configured
  `models:`; now the merged catalog (config-first + curated kind defaults,
  ollama.com providers get the hosted list) always feeds the dropdowns. This
  is also the Ollama split: a provider entry with `base_url: https://ollama.com`
  gets the cloud catalog, a local one stays free-text.
- `features/settings/presentation/MainModelPicker.tsx` — "Custom…" option in
  the model select for manually typing any model id even when a catalog exists.
- `shared/infrastructure/clipboard.ts` (new) — `copyText` with
  hidden-textarea fallback: `http://tauri.localhost` is not a secure context on
  Windows, so `navigator.clipboard` can be undefined and Copy ID / copy-code /
  copy-path silently threw. Export now attaches its anchor to the DOM before
  clicking.
- `features/shell/presentation/StatusBar.tsx` + `viewmodels/useStatus.ts` —
  removed the ticking session timer (user call) and with it a 1s re-render of
  the status bar.
- **Hotfix (same batch):** the first cut of the title backfill AWAITED
  `session.backfill_titles` inside the deacon's serial stdin dispatch loop —
  up to 30 sequential model calls queued every other request behind them and
  froze all settings pages on loaders. The op now replies `{started: true}`
  immediately and sweeps on a detached task
  (`dispatcher/session_admin_ops.rs`), announcing each landed title with the
  same `session.titled` notification first-turn titling uses
  (`session_manager/backfill.rs`) — the desktop's existing subscription
  patches rows live, no refetch.
- `features/settings/presentation/VoiceSection.tsx` — ASR/TTS model selection
  is now a dropdown fed by a curated per-provider map (groq/openai/mistral/
  elevenlabs ASR; openai/elevenlabs/minimax/gemini/edge TTS) with a "Custom…"
  free-text escape; providers without verifiable ids stay free text (same bar
  as the chat provider catalog).

Verified: `bun run typecheck` + `bun run build` green in
`src/regent-app/Desktop`; `cargo test -p regent-deacon --lib` 81/81 green;
`regent-deacon.exe` rebuilt (the running copy was renamed aside — restart the
app to pick it up).

## 2026-07-10 — desktop: Next.js → Vite 8 + React Router 7 (ADR-034)

**Goal:** drop the Next.js layer under the desktop app without changing the UI,
the routes, or anything on the Tauri/Rust side.

- The coupling was thin by design: six thin `page.tsx` wrappers, one layout,
  and `next/navigation` hooks in nine files. The cutover is one commit,
  +234/−226 lines, and `tauri.conf.json` is untouched — Vite serves dev on the
  same port 3000 and builds into the same `out/` dir Tauri already consumes.
- New: `vite.config.ts` (react + `@tailwindcss/vite`, `@` → package root),
  `index.html` (carries the theme no-flash script verbatim), `app/main.tsx`
  (StrictMode + BrowserRouter + the six-route table + `*` → home), and
  `shared/infrastructure/router/adapter.ts` — a 25-line Next-compat shim
  (`useRouter`/`usePathname`/`useSearchParams` over react-router-dom), so the
  nine call sites changed only their import specifier.
- Env seam: `NEXT_PUBLIC_SPEECH_URL` → `VITE_SPEECH_URL` (same localhost:8000
  fallback). Deleted: `next.config.mjs`, `postcss.config.mjs` (the Tailwind
  Vite plugin replaces PostCSS), the page wrappers, layout, stale `.next/`
  cache, and dead `@next/next` eslint-disable comments.
- Toolchain now latest across the board: Vite 8.1.4 (rolldown), TypeScript
  7.0.2 (native compiler — typechecks this tree clean), react-router-dom
  7.18.1, Tailwind 4.3.2, React 19.2.7.
- **Perf follow-up (same day):** the first cut statically imported all six
  route views + ButlerView, so first paint pulled a 1.87MB entry chunk and
  felt slow. `React.lazy` per route (the splitting Next did implicitly) plus a
  lazy Butler boundary (maplibre-gl out of boot) brought the entry to 322kB
  (gzip 96kB), 5.8× smaller. BootSplash overlays the Suspense gaps.

**Verified:** `bun run typecheck` green; `bun run build` (tsc + vite) emits a
clean SPA into `out/`; `bun run tauri dev` boots the window, Vite ready in
262ms. Known accepted risk: a hard reload while on a deep route in a *release*
build depends on Tauri's index.html fallback — release builds expose no reload
affordance, and the fix if ever needed is HashRouter behind the shim.

## 2026-07-10 — file-length remediation: top 8 offenders split

- The audit's eight largest files (998–402 lines) are now feature-seamed
  modules, each ≤ ~200 lines, with public APIs unchanged (module re-exports)
  and identical test counts before/after (all suites green):
  - `dispatcher/admin_ops.rs` → ten per-feature `*_ops.rs` (skills, memory,
    model/providers, mom, cron ×2, status, persona, kanban, agents);
    `dispatcher/voice_ops.rs` → voice_ops + voice_set_ops + voice_weights_ops
    + speech_yaml.
  - `key_tool.rs` → `key_tool/{mod,catalog,env_file}` (catalog.rs stays 224:
    one flat const table of managed keys).
  - `voice-server http.rs` → `http/{mod,security,pages,audio,call,tests}`;
    `speech remote.rs` → `remote/{mod,asr,tts,tests}`;
    `wechat.rs` → `wechat/{mod,media,tests}`.
  - Test monoliths became directory test crates: `deacon_basics/` (8 modules,
    32 tests) and `agent_loop/` (4 modules, 9 tests).
  - Details + remaining offenders: `docs/audits/2026-07-10-file-length-audit.md`.

## 2026-07-10 — live switching reaches open sessions; voice fillers instant; bug batch

- **Model/key/config changes now reach OPEN sessions** — sessions used to
  capture their provider at build, so switches applied only to new sessions.
  A routing epoch (bumped by `model.set` + the config/env reload path) makes
  `run_turn` swap in a freshly resolved provider when stale (new additive
  `Agent::set_provider`). Test: `model_switch_applies_to_open_sessions_next_turn`.
- **Compaction trigger displays clean** — `context.trigger_fraction` is f64
  end-to-end (deacon config, agent CompressionConfig, threshold math); the
  f32 0.85 → 0.850000023842-style display noise is gone at the source.
- **Voice fillers are instant** — the slow-first-token filler lines are
  pre-synthesized into a WAV cache after engines load; speaking one no longer
  pays TTS latency at the exact moment the call is bridging dead air.
- **Voice calls ignore other sessions' streams** — `RpcEvent` carries
  session_id (a background job / cron turn in the same deacon is never spoken
  into the call); 600s stall ceiling matches the turn loop; Python fallback
  emits keepalives. Plus `windowsHide` on CLI detached spawns and
  computer-use default-on in the CLI.
- **Model page consolidated** (incl. parallel-session work): ONE main model
  picker writing the canonical `agents_defaults.primary` (dead `model.*`
  write path deleted), shared key picker on main + fallback rows, fallback
  dedup (incl. rapid double-click gap), free-text model input for providers
  with no listed models (draft + commit-on-blur, no per-keystroke writes).
- **session.backfill_titles** — additive op titling untitled sessions with a
  real exchange (limit-bounded; {titled, skipped, remaining} reply); the ~900
  pre-titling sessions can now be named by repeated calls.
- **en.ts split** — 473-line i18n file becomes a 28-line barrel over 11
  domain files, all under 200 lines.
- **Every provider lists models** — new additive `providers.models` op:
  the provider's own `models:` entries lead, then its KIND's curated
  defaults follow (deduped) so one pinned id never hides the catalog.
  Every remote kind carries ≥5 current ids; OpenRouter carries 64
  org-prefixed slugs and an ollama-kind provider pointed at ollama.com
  gets the 15-model HOSTED catalog (local ollama stays machine-known).
  All ids verified against the LIVE catalogs on 2026-07-10
  (openrouter.ai/api/v1/models + org pages, ollama.com/search?c=cloud,
  Anthropic API reference). Never persisted into config.yaml; an older
  deacon without the op degrades silently to config-listed models.
- **Per-ref API keys (multi-key failover)** — `ModelRef` gains an optional
  `key_slot` (omitted when unset; `#N` in logs): the registry resolves that
  exact slot's var (`<BASE>_N`) and memoizes per slot, an unset slot falls
  through the chain as the usual MissingKey, and `config.set` bounds slots
  to the 8-slot max. UI: the key picker on the main row and on EACH
  fallback row now binds that row's slot (no more provider-global
  `env.activate` swap that silently changed the other rows' key), so the
  same provider+model on a different key is a legitimate fallback — dedup
  compares the (provider, model, slot) triple.
- **Higgsfield** joined the managed video-generation keys (it was absent
  from the MANAGED table, not just the stale binary). Kling + Higgsfield
  also row up under Image generation (`extra_key_groups` — one key, one
  env var, listed in every group its products cover).
- **Fallbacks exhaust by stored keys** — a provider+model supports one
  chain link per stored key: model options hide spent combos, the row's
  key options exclude slots other links use on the same combo, and
  "+ Add fallback" disables when every (provider, model, slot) triple is
  taken (keyless providers count one implicit slot).
- **Gateway keys editable in place** — the Gateway section reuses the API
  Keys row (env.set/unset, masked) instead of read-only presence.
- **Number inputs get in-chrome steppers** — native spin buttons (which
  overflowed the rounded field) are replaced by compact token-styled
  chevrons honoring step/min/max.
- **File-length audit** — `docs/audits/2026-07-10-file-length-audit.md`
  inventories the 96 Rust files over the ~200-line house target, worst
  first (deacon_basics.rs 998, dispatcher/admin_ops.rs 965, key_tool.rs
  562); splitting is queued follow-up work, not done in this batch.
- **Found, no code needed**: empty ctx popover + missing gateway/messaging
  key groups were the STALE release deacon binary (rebuilt; restart app).
  "Can't hear other apps on call" was Windows communications ducking —
  `HKCU\...\Audio\UserDuckingPreference = 3` set on this machine.
- **Token efficiency assessed** — memory prompt blocks are budgeted
  (2,200 + 1,375 chars); the ~25k average input is conversation history +
  tool results, which compaction only trims at `trigger_fraction` (0.85 ×
  200k by default; the user's config: 0.5). Lower it in Chat settings for
  leaner requests — no unbounded injection found.
- **Verified** — `cargo test` green on agent/deacon/voice-server (+ workspace
  `cargo check`); `bun run typecheck` + `bun run build` green; release
  deacon + voice-server rebuilt (`*.stale*` renames deletable once the app
  is closed; restart the app to load them).

## 2026-07-09 (later) — model switching works E2E; live config; the app fills out

- **Provider/model switching verified end-to-end** — `model.set` re-routes the
  fallback chain (picked "<provider>/<model>" becomes primary), user-pasted
  base URLs compose without doubling path segments (the OpenRouter HTML-404
  root cause), any 404 maps to one actionable sentence, and a model spec
  resolves to the provider that actually lists it (fixed chat AND butler
  dying after a switch).
- **Config/keys apply live** — a RwLock routing snapshot + ConfigReload hook:
  `config.set`/`env.set`/agent `manage_keys`/`voice.set` all hot-apply to the
  next session, schema gate unchanged. Voice server is a separate process —
  its model changes land on next Butler open.
- **Multi-key per provider** — numbered slots (`KEY_2…_8`), "+" add, rows
  collapse behind a chevron, `env.activate` swaps the active key; the Model
  page's Primary/Secondary/Fallback rows grow a Key picker. 30 new managed
  providers across image / video / sound generation groups.
- **Butler noise/latency** — server-side pre-ASR energy gate + whisper
  hallucination filter + barge-in cancellation of abandoned turns, tunable
  via `REGENT_VAD_*`. True full-duplex designed (streaming endpointer + AEC),
  not built.
- **ctx meter + slash commands** — `turn.complete` carries input/output/max
  tokens; `commands.list` covers the full 30-command CLI surface with an
  `executable` flag (incl. /learn).
- **The app fills out** — real Profiles page (SOUL separated + five editable
  About facets bound to `about.*` persona keys; constitution hidden by
  design), Artifacts viewer (collapsed slug groups), Messaging platform
  groups (all 17 platforms, collapsed), every settings section real
  (Chat/Workspace/Safety/Gateway/MCP/Archived), titlebar session menu
  ([New Conversation / {title}]: Pin, Copy ID, Export, Rename, Archive,
  Delete), Code page echoes the task instantly, mic-button hydration fix.
- **Open items** live in `docs/HANDOFF-2026-07-09.md`.

## 2026-07-09 — backend streaming + resume repair; grouped API keys; rail collapse

- **Real streaming for every OpenAI-compatible provider** — `openai_stream.rs`
  streams the SSE wire (content fragments → delta sink as they arrive; tool-call
  fragments accumulate by index; usage via `stream_options.include_usage`).
  Before, only Anthropic streamed — chat showed no typing animation on
  Ollama/OpenRouter/Groq/etc. Single attempt, no mid-stream retry.
- **Resume repairs crashed-turn history** — a failed turn leaves its rows in the
  store (dangling user message, unanswered tool calls); `Agent::resume` used to
  fail hard on replay ("transcript invariant violated: two user messages in a
  row"), bricking those sessions. Replay now applies the same recovery
  `run_turn` uses live, skips a still-illegal row, and trims the tail.
- **Fallback chain verified + test gap closed** — interactive chat builds
  `FallbackChat` from `agents_defaults` via `provider_registry::chain_for`;
  new tests: 429 fails over, 4xx does not, streaming fails over only before
  the first delta. MOM (`mom.run`) verified aggregating proposer outputs.
- **API Keys page covers ALL managed key types** — `env.list` now returns the
  full MANAGED set with an additive `group` field (llm/messaging/search/speech);
  the settings page renders collapsible group panels. Values stay masked.
- **Token budget measured + trimmed** — repeatable breakdown in
  `tests/token_budget.rs` (prompt 4091 + 30 tool schemas 4723 chars/4);
  camera_capture, vision_analyze, delegate_task, send_message join the default
  deferred list: model-facing input 7604 → 7142 per call, tools still load on
  demand.
- **Rail collapse** — the titlebar panel button now slides the left rail
  closed/open (200ms motion-safe width animation, content clipped not
  reflowed). SESSIONS group collapsed by default, showing the 7 newest; the
  session list scrolls separately under a fixed nav head; new sessions appear
  live (refetch on unknown `turn.started`).
- **Chat feel** — instant user-message echo on send (was delayed by
  session.create), pending dots render in the reply's slot from the moment of
  send (reasoning models think silently first), THINKING rows collapsed by
  default, seeded tool chips render before their row's text, slash `/` menu in
  the Code page via a shared `useSlashMenu` hook.
- **Window/pan + inputs** — `overflow: clip` on html/body + shell kills the
  whole-window horizontal swipe (hidden still allowed focus-induced pans);
  dropdowns get a token-tinted chevron (native glyph invisible on dark);
  text inputs lose the focus rectangle; dark-mode code blocks use Shiki's dual
  theme; the `regent` tool description teaches the real name-keyed provider
  schema (17 kinds + agents_defaults ModelRef examples).
- **Status-bar popovers** — gateway, agents, cron, and context % each open a
  small panel (shared StatusBarPopover primitive, outside-click + Esc dismiss)
  showing model/sessions/cron state and token usage — all from existing RPCs.
- **Skills & Tools grouped redesign** — search + Skills/Toolsets tabs +
  "All"-first category chips with counts + grouped sections with per-row
  toggles, replacing the master-detail layout (Hermes parity).
- **Verified** — `cargo test -p regent-deacon -p regent-providers -p
  regent-agent -p regent-tools` green; `bun run typecheck` + `bun run build`
  green; fresh release deacon driven over stdio (status/list/history) plus
  in-app chat E2E against Ollama Cloud.

## 2026-07-09 - desktop app: Code handoff, session search, mic dictation + polish

- **Normal chat to Regent Code handoff** - text-only prompts that look like
  coding tasks now redirect from the regular chat composer to `/code?task=...`
  instead of starting a normal chat turn. The Code page consumes that task once,
  immediately starts `code.plan`, and then cleans the URL back to `/code`.
- **Regent Code landing polish** - the Code page now shows the centered
  `REGENT CODE` wordmark, keeps the title fixed while the task input expands,
  and now renders the same full Composer used by regular chat, including the
  attach button, mic control, model pill, circular send/stop button, and
  coding-specific placeholder.
- **Shared seven-line prompt bar** - extracted the rounded prompt input into a
  shared `PromptInputBar` used by both regular chat and Regent Code. The input
  now auto-sizes from one line up to seven visible wrapped lines, then scrolls
  internally, so long prompts no longer push the surrounding hero/title layout.
- **Softer prompt shadow** - the shared chat/Code input bar now uses a lighter
  prompt-specific shadow token, reducing the heavy drop shadow without changing
  other elevated overlays or cards.
- **Light-mode prompt depth** - increased the light-theme prompt shadow for
  both chat and Regent Code input bars so they read as floating surfaces
  without changing the dark-mode shadow.
- **Chat composer lift** - raised the regular chat input bar to match Regent
  Code's bottom breathing room and expanded transcript padding so streamed
  messages still clear the floating composer.
- **Session rail search fixed** - the search field above Sessions is now a real
  controlled filter over session title, source, model, id, and message count.
  Search reveals matches even when the Sessions or Archived groups are
  collapsed, and it shows a clear no-matches message.
- **Chat mic speech-to-text** - the regular chat mic button now starts/stops a
  push-to-record flow with live text preview in the composer while speaking.
  It reuses browser interim recognition when available, falls back to periodic
  local transcription previews, then settles the final transcript through the
  desktop voice-server lifecycle by converting the browser recording to 16 kHz
  WAV and sending it to the local OpenAI-compatible
  `/v1/audio/transcriptions` endpoint without auto-sending.
- **Home hero spacing** - tightened the gap between the main `REGENT` wordmark
  and subtitle so the empty chat state reads as one compact lockup.
- **Verified** - `npm.cmd run typecheck`; `npm.cmd run build`.

## 2026-07-07 — desktop app M9: dark theme, chat embeds, notifications + keybinds

- **Dark theme** — a real designed dark ramp (warm-charcoal base off the bone
  hue, brand teal kept; text ramp clears WCAG AA on the dark bg), not an
  inversion. `:root[data-theme="dark"]` + a `prefers-color-scheme` default for
  an un-chosen first run; an explicit Light/Dark choice always wins. Theme
  store on `useSyncExternalStore` (persists to localStorage, stamps `data-theme`
  on `<html>`), a no-flash inline head script, and a real **Appearance**
  settings section (Light/Dark/System).
- **Chat embeds (consent-gated)** — YouTube (privacy `youtube-nocookie`) and
  OpenStreetMap links render a placeholder card; the iframe mounts only after
  the user clicks Load. Fenced ```mermaid``` blocks render to local inline SVG
  (`securityLevel: 'strict'`, falls back to the raw code on parse error). CSP
  extended minimally: `frame-src` youtube-nocookie + openstreetmap only.
- **Notifications + chime + keybinds** — on a turn completing while the window
  is unfocused, a native notification (Tauri notification plugin) fires and a
  ~150ms WebAudio chime plays (no binary asset); silent when focused. A
  keybinds panel (opened with `?`) lists the shortcut map.
- **Live config fix** — corrected a user config.yaml the agent had bricked
  before `config.set` existed (it had written `providers` as a list with
  `type`/`priority`/`model` and provider `ollama-cloud`; the schema wants a
  name-keyed map with `kind`/`api_key_env`/`models` and enum `ollama` +
  `base_url`). Rewritten to a validated equivalent (verified by loading it in
  the release deacon); the broken file is backed up alongside it.
- **Verified** — `tsc` clean, `bun test` 7/7, `next build` (8 static pages),
  `cargo build` (Tauri crate) green with the notification plugin.

## 2026-07-07 — desktop app M8: chat parity, attachments + config-write safety & skills fixes

- **Chat surface** — Shiki code highlighting (offline JS engine, no CDN),
  expandable long code blocks, click-to-zoom images. Composer v2: model
  hot-swap pill, `/`-command completions, input history (↑/↓), scroll-to-
  bottom button, live turn timer.
- **Attachments end-to-end** — attach button → `attachment.put`
  (`$REGENT_HOME/attachments/<session>/`, 20 MB cap, traversal-safe) →
  `prompt.submit {attachments}` appends file refs the agent's tools read.
  Staged chips with remove; upload failure aborts the send verbatim.
- **Shell** — session rail actions (rename/pin/archive/delete) with
  Pinned + collapsed Archived groups; status bar v2 (model menu panel,
  cron/agents counts, context-% meter fed by turn usage); boot-failure
  overlay when the deacon is dead/never spawned. `session.titled`
  updates the rail live.
- **Deacon (additive)** — `attachment.put`; first-user-turn title
  generation (one cheap aux call → `store.rename_session` → `session.titled`
  notification, never blocks the turn); `input_tokens`/`output_tokens`/
  `context_max` on `turn.usage`; per-turn token tracking in `regent-agent`.
- **Config-write safety (user-reported fatal)** — the agent had no validated
  path to change config, so it hand-edited config.yaml and wrote
  `provider: ollama-cloud`, an invalid enum that made startup fatal. New
  **`config.set {path, value}`** sets a dotted path then deserializes the
  WHOLE file into `DeaconConfig` (the exact startup type) before writing —
  an invalid enum/typo/wrong-type is rejected with that verbatim error and
  the file is left untouched. The `regent` tool now routes all config edits
  through it and is told never to hand-edit config.yaml. To recover the one
  already-broken file, set its `model.provider` back to `ollama`.
- **Skills fixes** — `skills.view` falls back to the archive so a listed,
  opted-out skill opens instead of erroring "skill not found"; Skills
  overlay tabs no longer collide with the overlay's close button.
- **Verified** — `cargo test` deacon 63 / skills 6 / agent green;
  `config.set` reject/accept + batch-A smoke against the rebuilt release
  deacon; `bun test` 7/7; `tsc` clean; `next build`.

## 2026-07-07 — desktop app M7: event bus, overlays, settings kit, skills restyle + deacon RPC batch A

- **Shared deacon event bus** (`shared/state/deaconBus.ts`) — ONE
  `deacon-event` subscription fanning out to subscribers + store slices
  (per-session turn activity, last error) on a tiny `useSyncExternalStore`
  store (no new deps). `useChatSession`/`useCodeRun` migrated.
- **Overlay framework** — Settings/Skills/palette now float OVER the live
  chat (Hermes overlay-view chrome: blurred scrim, near-fullscreen inset
  card, top-right ×, Esc/scrim dismiss); `/settings` + `/skills` routes
  removed, chat never unmounts.
- **Settings kit + the dead Voice actions fixed** — primitives
  (Section/FieldRow/TextField with dirty-armed centered Apply), rail
  search with field keywords. Root cause of the user-reported breakage:
  the voice pickers list PROVIDERS but wrote them into the `model` config
  key; they now call the new `voice.set {asr_provider|tts_provider}`, with
  models + whisper size as their own fields.
- **Skills & Tools restyle** — top search, Skills/Tools tabs with counts,
  tag/toolset chips, enable/disable switches (`skills.opt_out` /
  new `skills.opt_in`; archived rows stay listed via
  `skills.list {include_archived}` and render dimmed). Tool switches are
  display-only until a config-write RPC exists.
- **Deacon RPC batch A (additive)** — `session.rename/pin/archive/delete`
  (+ `title/pinned/archived` on `session.list`), `skills.opt_in` +
  archived listing, `voice.set` provider params, `args_summary` /
  `result_summary`+`ok` on `tool.start`/`tool.complete`.
- **Butler mic UX** — mic-denied and the silent-mic watchdog now auto-open
  `ms-settings:privacy-microphone` (scoped opener grant) instead of
  describing the Settings path; Windows cannot re-summon its permission
  popup once blocked.
- **Verified** — `cargo test` deacon/skills/store green (new roundtrip
  tests) · batch-A RPCs smoke-tested live against the rebuilt release
  deacon in a throwaway home · `bun test` 7/7 · `tsc` clean ·
  `next build` clean (routes gone as designed).

## 2026-07-06 — desktop app M3c+M4+M5: map/insights windows, Code page, real Settings

- **M4 Code page** (rail + ⌘K + `/code`) — regent-code's flow end-to-end:
  bottom composer-style task input → `code.plan` renders the plan as
  markdown → Approve & run → `code.start` streams the live run log (tool
  rows, approval cards — the shared Transcript) → verify passed/failed
  badge, report, reverted notice.
- **M3c Butler windows** — dock grows to Conversation · **Map** (MapLibre
  over OSM raster tiles, Nominatim place search; CSP extended to exactly
  those two origins + worker blob for the GL worker) · **Insights** (token
  in/out bars + session/turn/message counters from `insights.get` — real
  data, no fakes).
- **M5 real surfaces** — Settings (Model get/list/set · Voice status/models/
  set · Memory list/pin/forget + pending approvals · About; unbuilt Hermes
  sections show honest roadmap states), Skills & Tools (skills.list/view +
  tools.list), Cron (list/toggle/run/remove), Profiles (SOUL editor over
  persona.get/set). Sonnet subagent built most of it (hit its session limit
  at the very end; cron/profiles routes + palette finished in-session).
- **Markdown everywhere** — assistant/thinking text through react-markdown
  +GFM on tokens; links open in the system browser (opener plugin).
- **Motion recalibrated to the Hermes rule** — reduced-motion (incl. Windows
  "Animation effects" OFF) now suppresses only movement; fades stay: loader
  swing, control transitions, voice-dot levels. Full motion (rises/settles,
  route transitions, row entrances) when animations are on. This was the
  root cause of every "nothing animates" report.
- **Verified** — `bun test` 7/7 · `tsc` clean · `next build` 9 static routes ·
  transport/token/layering audits clean · `tauri build --no-bundle` green.

## 2026-07-06 — desktop app: per-turn transcript, approvals, computer-use parity + M3b windows

- **Per-turn transcript (Hermes-style)** — chat now renders the turn's
  structure, live and from history: quiet **Thinking** blocks (stored
  `reasoning`), **tool rows** (wrench + name, spinner while running, error
  tint on failure — live via the deacon's existing `tool.start`/`tool.complete`
  events), and the final reply consolidates streamed fragments (no
  duplication when deltas interleave with tools). `session.history` rows now
  carry `reasoning` + `tool_calls` (additive).
- **Approvals in chat** — `approval.request` renders an Approve/Deny card;
  the answer goes back over `approval.respond`. Without this, every mutating
  computer-use action silently hung 120s then denied — the actual "computer
  use doesn't work" failure mode on surfaces with no approval UI.
- **Computer-use parity** — the desktop's deacon spawn now defaults
  `REGENT_COMPUTER_USE=1` like regent-cli and the voice server (real env /
  `.env` still win). CLI chat, `regent call`, and the desktop are in unison;
  stale `regent-deacon`/`regent-voice-server` processes killed and both
  release binaries rebuilt (the regent-web/`regent call` breakage was stale
  binaries — their fixes were already in source).
- **Resume UX** — clicking a session shows a Loader (no more dead wait);
  `session.resume` + `session.history` fetch in parallel.
- **M3b (Butler windows, first slice)** — React Spring lands: draggable
  floating windows (borderless + shadow, header drag, click-to-front,
  remembered positions) with a live **Conversation** window fed by the call's
  caption log, toggled from a dock chip. Snap-docking + more window types
  come with real agent-driven content.
- **Verified** — `bun test` 7/7 · `tsc` clean · `next build` clean ·
  bridge `cargo test` 3/3 · `cargo test -p regent-deacon` 56/56 ·
  `tauri build --no-bundle` green.

## 2026-07-06 — desktop app: braille voice mark, real history on resume, stuck-call fixes

- **Butler visual** — the 3D ring is out; Regent's mark is the regent-web
  **braille dot field** (`VoiceDots`, faithful port of `BrailleVoiceViz`:
  canvas 60fps from the analyser, GSAP-breathed idle floor), recolored from
  the `--accent` token and given a **radial alpha falloff** so the field
  melts into the grid instead of ending in a hard rectangle (the source sat
  on a dark page). Butler icon is now the real mark from
  `assets/ButlerModeIcon.svg` (cloche-and-hand), not a generic audio glyph.
- **Fix (stuck on "Listening")** — WebView2 can hand back a **suspended**
  AudioContext when creation happens seconds after the opening click (server
  probe + mic prompt consume the gesture window); a suspended graph = dead
  VAD + frozen visualizer. The context is resumed on creation with a
  one-shot pointer/key fallback, and the source file's peakRMS `console.debug`
  diagnostics are restored (dropping them was a mistake — they exist to
  debug exactly this).
- **Feature (deacon, additive)** — new `session.history` RPC: stored
  user/assistant transcript rows (`store.get_conversation` was already
  there, never exposed). Desktop seeds a resumed session's transcript from
  it — clicking a rail session now shows past messages. Seed never clobbers
  live turns (reducer guard + test). Deacon: 27/27 tests green, release
  binary rebuilt (running process must be killed for the new method to be
  live — done).
- **Verified** — `bun test` 5/5 · `tsc` clean · `next build` clean ·
  `cargo test -p regent-deacon` 27/27 · `tauri build --no-bundle`.

## 2026-07-06 — desktop app: hydration fix + UI polish pass (user feedback)

- **Fix** — React hydration mismatch on boot: `useSessions` initialized
  `loading` from `isTauri()`, so the static prerender and the first in-shell
  render disagreed (Loader vs empty-state). Initial state is now
  environment-independent; the shell check moved into the effect.
- **Design pass** (vs Hermes reference): watermark no longer stacks under the
  hero (it was rendering as a second giant REGENT — now it appears only behind
  live transcripts, fainter, neutral-toned); composer rebuilt as a floating
  rounded surface with a circular send/stop (stray orb dot dropped); thin
  quiet scrollbars app-wide; denser session rows (`dense` ListRow); main pane
  sits on `--surface` for depth against the rail.
- **Nothing dead-ends** — Skills & Tools / Messaging / Artifacts / Settings now
  route to titled placeholder pages (EmptyState "on the roadmap"), wired from
  the rail, palette, and the titlebar gear.
- **Butler** — setup failures (voice server unreachable/CORS-ungranted, mic
  denied) now render as the full centered error block instead of a caption.
- **Verified** — `bun test` 4/4 · `tsc` clean · `next build` clean (6 static
  routes) · `tauri build --no-bundle`.

## 2026-07-06 — desktop app M3a: Butler Mode (voice call core)

- **Feature** — full-screen "Jarvis" view on the existing voice stack: the
  titlebar audio button opens it; Esc/X exits (mic + audio graph torn down).
  Kinetic Three.js particle core (teal ring, ~2.6k points) breathes when idle
  and swells with speech amplitude — the mic and the reply audio feed one
  analyser; GSAP glides the per-phase energy (idle → listening → thinking →
  speaking). Token-tinted drifting grid field behind it, radially faded.
  Live captions: phase · heard · reply · errors verbatim.
- **Ported, not reinvented** — the VAD turn loop (`callLoop.ts`) and speech
  I/O (`speechClient.ts`) are near-verbatim ports of regent-web's
  battle-tested `localCall.ts`/`speechServer.ts` (same thresholds, barge-in,
  noise floor, hung-turn watchdog, keepalive handling). Camera frames
  (`/call/frame`) deliberately not ported yet.
- **Voice-server lifecycle** — webview probes `:8000/health` (CSP-allowed);
  when down, the new `voice_spawn` Tauri command launches the prebuilt binary
  detached + hidden with the CLI's env contract, reused across runs. CORS:
  the server only grants one configurable extra origin, so the spawner sets
  `REGENT_CALL_UI_ORIGIN=http://tauri.localhost` (real env wins); a server
  started by the CLI without that grant surfaces an actionable error instead
  of dead fetches. Deacon spawn env refactored to a pure `merged_env` shared
  by both spawners.
- **Deps** — `three` + `gsap` land now (first use, per install-at-first-use);
  Lenis still not needed.
- **Verified** — `bun test` 4/4 · `tsc` clean · `next build` clean ·
  `cargo test` 3/3 · audits clean · `tauri build --no-bundle` green.
  Reduced-motion: static ring, no grid pan (global kill).

## 2026-07-06 — desktop app M2: live chat on the deacon turn stream

- **Feature** — Home is now the chat surface: hero empty-state → composer;
  first submit lazily runs `session.create`, subscribes to that session's
  events, then `prompt.submit`. Streaming transcript (deltas accumulate, seal
  on `turn.complete`), stop button → `turn.interrupt`, rail sessions route
  into `/?id=` and resume (`session.resume`). Composer: auto-growing textarea
  (Enter sends, Shift+Enter newline), attach/mic placeholders, pulsing
  voice-orb placeholder (real orb lands M3), ≥44px send/stop.
- **Fix (M0 bridge)** — `prompt.submit`'s JSON-RPC response only resolves when
  the whole turn ends, but the Rust bridge timed every request out at 30s — a
  >30s turn would spuriously fail the invoke mid-stream. Turn-length methods
  (`prompt.submit`, `code.plan`, `code.start`, `mom.run`) now get 630s
  (deacon's 600s stall ceiling + slack); everything else keeps 30s. The
  viewmodel also ignores `-32000` response errors (already delivered as
  `turn.complete {error}` — no duplicate error rows) and surfaces provider
  errors verbatim (the deacon pre-humanizes 401/402/429).
- **Known ceilings** — resumed sessions start with an empty transcript
  (session history isn't exposed over RPC yet; additive `session.history`
  later); rail rows show `source · short-id` (no titles on the wire);
  thinking/tool-call rows need wire events that don't exist yet (M2.5+).
- **Verified** — `bun test` 4/4 · `tsc` clean · `next build` clean ·
  `cargo test` 3/3 warning-free · audits clean (no transport in presentation,
  tokens only) · `tauri build --no-bundle` rebuilt.

## 2026-07-06 — desktop app M1: shell + design-system primitives

- **Feature** — the app frame: frameless-window titlebar (drag region, native
  min/max/close through a `shared/infrastructure/window` seam — presentation never
  imports `@tauri-apps/api`), left rail (Hermes IA renamed: New session · Skills &
  Tools · Messaging · Artifacts · search · Pinned · Sessions — live over
  `session.list`), status bar (gateway dot + model from `status.get`/`model.get`,
  ticking session timer, version; unfed slots show "—", no fake data), ⌘K command
  palette (Esc closes, focus restored), faint full-bleed watermark.
- **Design system** — `shared/ui`: Button (5 variants incl. titlebar zone),
  SearchField (borderless, underline-on-focus), ListRow, Loader (never literal
  "Loading…"), ErrorState (provider errors verbatim — 401/402/429 never masked),
  EmptyState, inline-SVG icon set. Tokens finished in `globals.css` (AA-fixed warm
  ramp, `--danger`, `--scrim`, global focus ring, reduced-motion kill-switch) —
  zero raw colors outside that file (grep-audited).
- **Chat core (M2 prep)** — `features/chat/domain/transcript.ts`: pure reducer over
  the verified wire events (`message.delta`/`message.complete`/`turn.*`), 4 unit
  tests green (`bun test`). Wire contract documented in the task plan; caught that
  the deacon namespace is `session.*` (singular), not `sessions.*`.
- **Note** — the M1 Opus subagent hit its session limit mid-run (finished only the
  token layer); the shell/primitives were completed in the main session instead.
- **Verified** — `bun test` 4/4 · `tsc --noEmit` clean · `next build` static export
  clean · dependency-direction audit clean (no cross-feature imports, no transport
  in presentation, domain imports nothing) · `tauri build --no-bundle` rebuilt with
  the M1 assets.

## 2026-07-06 — desktop app M0: Next+Tauri scaffold on the deacon seam

- **Feature** — greenfield `src/regent-app/Desktop/`: Next 16 static-export UI inside
  a Tauri v2 shell. The Rust core spawns `regent-deacon` hidden (stdio JSON-RPC — the
  same transport as the CLI and voice server, ADR-033) and bridges it to the webview
  via one validated `deacon_request` command + `deacon-event` events that preserve
  `session_id` for client-side filtering. Least-privilege webview: no shell/fs
  capability, CSP locked to self + the voice-server port (Butler Mode later).
- **Design** — tokens-only styling (`--bg` warm bone / `--accent` teal + derived warm
  ramp in `globals.css`); KONTES Compressed Bold self-hosted behind one `@font-face`
  swap point with a condensed fallback stack — the font file is **personal-use
  licensed** and gitignored (fresh clones silently fall back; commercial distribution
  needs the paid license). Placeholder teal-R icon set (swap at M6).
- **Plan review** — `Regent-Desktop-TASK.md` corrected before build: `regent-web` was
  misidentified as a backend (it's a call-page client); Butler split M3a/b/c;
  clean-architecture trimmed to a thin-RPC-client shape; deps install at first use;
  auto-update deferred. GATE confirmed: deacon seam · font provided · light-first ·
  placeholder brand.
- **Verified** — `bun run build` (static export clean) · `cargo test` in `src-tauri`
  3/3 green incl. a real-deacon `status.get` round-trip against
  `target/release/regent-deacon.exe` · `tauri build --no-bundle` →
  `src-tauri/target/release/regent-desktop.exe` launches.

- **Feature** — new `background_task` tool in the deacon: building software, deep
  research, producing documents/spreadsheets/decks now run as a **detached
  full-toolset agent session**. The tool returns immediately ("started — I'll
  report back"), so a live call keeps flowing and barge-in no longer cancels the
  job. Results and running-status are injected into the **next real turn** on any
  surface (voice, CLI chat, HTTP, gateway platforms) and the model relays them
  naturally; delivered once, then cleared. The voice system prompt routes
  long jobs (incl. `code_task`) through it on calls.
  (`regent-deacon/application/background_task_tool.rs`, `session_manager/lifecycle.rs`
  `run_detached_task`, injection in `dispatcher/session_ops.rs` + `http_serve.rs`.)
- **Fix** — voice server now filters streamed RPC events by **session id**
  (`RpcEvent::Delta/Reply/End` carry `session_id`): a background job's or cron
  turn's deltas can no longer be spoken into a live call.
  (`regent-voice-server/domain/rpc.rs`, `infra/deacon.rs`.)
- **Change** — turn stall ceiling raised 180s → **600s** in both the voice-server
  turn loop and its deacon RPC client (a deep search that streams nothing for
  minutes is legit; keepalives bridge the client meanwhile). Python fallback
  matched.
- Not built (say the word): live mid-call "it's done" announcement (needs a
  client poll channel — today results arrive the next time you speak).

## 2026-07-06 — call: stale-binary "took too long" resets + hidden voice server

- **Fix** — the 7/04 keepalive fix never reached users: `regent call` prefers
  `target/release`, whose binary predated the commit, and reuses any
  already-running `:8000` server forever. Rebuilt; **after voice-server changes,
  rebuild release AND kill the running process**. The Python fallback server also
  gained the keepalive loop it never had (`web_call.py`).
- **Fix** — the voice server spawned by `regent call` opened a visible console
  window on Windows (detached without `windowsHide`); closing it killed the
  voice mid-call. Now spawned hidden (`voiceServe.ts`, browser-open flash in
  `callServe.ts` too). Stop it with `Stop-Process -Name regent-voice-server`.

## 2026-07-06 — computer use: DPI-correct clicks, no focus-stealing flash, CLI default-on

- **Fix** — the PowerShell backend was DPI-unaware: on a scaled display
  (125%/150% — most Windows laptops) screenshots came out logical-size while
  clicks landed in virtualized coordinates, so the model aimed at what it saw
  and missed. Every script now calls `SetProcessDPIAware` first.
- **Fix** — each action spawned a console `powershell.exe`/`cua-driver` with no
  `CREATE_NO_WINDOW`: under the (now hidden) deacon it popped a console that
  also **stole focus from the target window right before SendKeys fired**.
  Both backends now spawn windowless.
- **Change** — the CLI (`regent chat`) now enables `REGENT_COMPUTER_USE=1` by
  default at deacon spawn, matching the voice call's default — safe because the
  TUI's interactive approval still gates every mutating action (voice
  auto-approves; that behavior is unchanged). Opt out with
  `REGENT_COMPUTER_USE=0`. (`regent-cli/shared/infrastructure/deacon/spawn.ts`.)
- Known ceilings (unbuilt): no scroll, double-click, right-click, or drag
  actions; Win-key combos silently drop the modifier (SendKeys limitation);
  primary monitor only.

## 2026-07-03 — call: keepalives during long thinks (no more "took too long" on memory queries)

- **Fix** — the watchdog cutoff could still fire on a genuinely long agentic turn
  (e.g. "what did we do yesterday?" → memory search + reasoning). The voice server
  spoke **one** filler on a slow first token, then waited up to 180s emitting
  nothing — so a >~20s silent think tripped the client's hung-turn watchdog even
  after the speak-time fix. The server now emits a silent `keepalive` line every
  8s while the brain is still working (one spoken filler first, keepalives after);
  the client already resets its watchdog on any streamed line, so a legit long
  turn is never mistaken for a dead one. A real stall still ends the turn after
  180s of continuous silence. (`regent-voice-server/application/turn.rs`; `cargo
  test -p regent-voice-server` green.)
- **Deploy note:** needs a rebuild of **both** `regent-voice-server` and
  `regent-web` (the earlier client silence-watchdog fix ships in the web bundle,
  and the running `.next` build predates it) + a hard reload of the call page — a
  stale build keeps the old cutoff behavior.

## 2026-07-03 — call: surface a missing TTS engine instead of dead air

- **Fix** — a turn on the live voice server with no TTS engine loaded streamed
  reply *text* but no audio, silently (the caller got dead air with no reason).
  It now emits a one-time "TTS unavailable — replying in text only" error up
  front, mirroring the ASR-missing path.
- **Context** — the audit's Surface-6 `SpeechIo`-returns-silence concern was
  about the legacy `regent-realtime` crate, which is **orphaned dead code**
  (nothing depends on it). The live `regent-voice-server` already surfaces
  ASR-missing, TTS-synthesis, and provider failures as errors; this closes the
  one remaining silent-degradation on it. `cargo test -p regent-voice-server`
  green (16 tests).

## 2026-07-03 — hardening: wire-parse panic-surface triage (production paths clean)

- **Triage (audit P3-class)** — reviewed every wire/JSON-parse `.unwrap()` in the
  hot crates (providers, deacon, voice-server, tools) for panic-on-runtime-input
  risk. Result: **effectively none on live request paths** — production parsing of
  untrusted input already uses `Result`/`?`/graceful fallback; the flagged
  `.unwrap()`s are almost all in `#[cfg(test)]` code (or a scripted-mock test
  server). The "~1054 unwraps" headline was dominated by tests and idiomatic
  `Mutex::lock().unwrap()`, not input-panic risk.
- **Fix** — the only production wire-parse unwraps were 3 infallible static-string
  header parses in the voice server's CORS middleware; converted to the canonical
  panic-free `HeaderValue::from_static(...)` (`regent-voice-server/infra/http.rs`).

## 2026-07-03 — security (W2.4 Layer A / P1-004): per-user inbound rate limiter

- **Fix (P1)** — no ingress plane had an inbound rate limit, so a paired-but-
  abusive user (or a burst) could flood the agent. A new `RateLimiter` (per-user
  token bucket, in the gateway lib like `AuthPolicy`) now gates every turn on the
  gateway runner, the deacon webhook (both sync + async paths), and Discord
  interactions — after authz, before any work. Over-limit senders get a "slow
  down" reply; no turn runs.
- Configured via `REGENT_MESSAGES_PER_MIN` (per user; unset/0 = unlimited, the
  default), read by both the gateway bin and the deacon HTTP listener — one knob,
  like the auth env config. In-memory/per-process (a restart resets buckets; two
  processes don't share state — a flood brake, not an accounting ledger).
- Guards: `rate_limited_sender_is_told_to_slow_down_and_runs_no_extra_turn`
  (webhook) + `allows_up_to_capacity_then_denies` / `zero_is_disabled` (limiter).
  With Layer B (per-turn token ceiling), **P1-004 is fully closed**. `cargo test
  -p regent-gateway -p regent-deacon` green.

## 2026-07-03 — security (W2.4 Layer B / P1-004): per-turn token spend ceiling

- **Fix (P1, partial)** — the agent loop had no cost ceiling: one message could
  drive up to `max_iterations` (90) model calls, each re-sending the full
  context, with no bound on tokens — unbounded API spend per message. The loop
  now sums `prompt + completion` tokens across a turn's calls and halts (like
  `max_iterations`) once the running total reaches `AgentConfig::max_turn_tokens`,
  logging the breach. Configured via a new `limits.max_turn_tokens` block in
  config.yaml, threaded into every session's `AgentConfig`.
- **Default off** (`None`) — no behavior change until an operator sets
  `limits: { max_turn_tokens: N }` to cap per-message spend.
- Guard: `token_ceiling_halts_the_turn_before_max_iterations` (a 20-token ceiling
  halts a runaway loop after 2 calls / 30 tokens, before the 90-step ceiling).
  `cargo test -p regent-agent -p regent-deacon` green.
- **Still open — W2.4 Layer A:** a per-user inbound rate limiter at the
  webhook/gateway boundary (anti-flood). This ceiling caps a single turn's cost;
  the rate limiter would cap request *frequency*.

## 2026-07-03 — security: constant-time signature checks on the WeChat/WeCom/Feishu paths

- **Hardening** — the WeChat and WeCom signature checks and Feishu's
  plaintext-mode verification-token check compared the computed value to the
  attacker-supplied one with plain `==`/`!=` (a timing side-channel), while every
  other webhook adapter already used a constant-time compare. Added
  `wechat_crypto::ct_eq` and routed all six sites through it (Feishu reuses its
  existing `feishu_crypto::ct_eq`). No behavior change for valid requests — closes
  the one finding from the per-adapter signature-crypto scan (the verifier layer
  W1.1 authz sits on). `cargo test -p regent-gateway` green (91 tests, incl. the
  WeChat/WeCom/Feishu verify paths + `ct_eq`).

## 2026-07-03 — security (W1.1 / P0-001): per-user authorization on the webhook + Discord planes

- **Fix (P0)** — a signature-valid webhook/Discord request ran a full agent turn
  for **any** sender; the allowlist/pairing `AuthPolicy` was only consulted by the
  polling gateway. The deacon's webhook handler (`infra/webhook.rs`) and the
  Discord interactions route (`infra/discord_interactions.rs`) now gate every turn
  on `AuthPolicy::is_authorized("{platform}:{user_id}")` — **default-deny**: an
  unknown sender's only capability is redeeming a one-time pairing code (persisted
  atomically to `gateway-auth.json`), never running a turn. Reuses the gateway's
  proven policy (the deacon already depends on `regent-gateway`).
- **Generalized auth config** — `load_auth_snapshot` / `persist_auth_snapshot`
  moved from the gateway bin into the gateway **lib** (one source for both planes).
  Operators are now allowlisted via `REGENT_ALLOW_ALL` + `REGENT_ALLOWED_USERS`
  (comma-separated `platform:id`, e.g. `slack:U1,discord:42`); the legacy Telegram
  vars still work as aliases. Persistence is atomic (tmp + rename).
- **⚠ Breaking (intended):** with no allowlist, no `REGENT_ALLOW_ALL=1`, and no
  pairing, the webhook plane now denies everyone (pairing prompt, no turn) — that
  open-bot behavior was the bug. Migration: `REGENT_ALLOW_ALL=1`, allowlist
  operators, or pair.
- Guards: `unauthorized_sender_gets_pairing_prompt_and_runs_no_turn` (webhook —
  proves a signed-but-unauthorized sender gets the prompt and no turn runs) and
  `persist_then_load_round_trips_paired_users` (gateway). `cargo test -p
  regent-gateway -p regent-deacon` green. (ADR-030.)

## 2026-07-03 — call: stop cutting off long spoken replies

- **Fix** — the web call loop's hung-turn watchdog was counting *speaking*
  time as "busy" and resetting the call at ~20s (`busyFrames > 235`), so any
  reply longer than ~20s of think+speak was guillotined mid-sentence with
  "That took too long — I reset. Try again." The watchdog now measures
  **silence**, not busyness: `busyFrames` resets on every streamed line from
  the server (`runTurn` → `onProgress`) and while audio is actively playing
  (`playing.src`), so it only trips on a real ~20s stall (dropped stream),
  never during a long, progressing reply. (`regent-web/hooks/localCall.ts`;
  `tsc --noEmit` clean.)

## 2026-07-03 — telemetry: make the input-token split legible (cache reads ≠ full-price input)

- **Diagnostic** — the Anthropic usage parser rolls `input_tokens +
  cache_read_input_tokens + cache_creation_input_tokens` into one `prompt_total`
  (correct for context-window/compaction accounting, but it makes a warm turn's
  mostly-cached prefix look like a large full-price bill). Both the non-streaming
  and streaming paths now emit a `debug` log splitting `uncached_input`
  (full price) from `cache_read` (~0.1×) and `cache_write` (~1.25×), so the real
  cost of a ~15k `prompt_total` is visible at a glance. No change to the returned
  `TokenUsage` (context accounting still sees the full prefix), the system/
  constitutional prompt, or output quality. Run with `RUST_LOG=debug` to see it.
  (`regent-providers/infra/anthropic/{response,stream}.rs`; `cargo test -p
  regent-providers` green.)

## 2026-07-02 — security/perf remediation wave + standalone build, auto code-routing, camera vision, deferred tools, doc-forge

**Remediation plan executed** (from `docs/audits/2026-07-02-remediation-plan.md`):
- **W3.3 perf** — store reads no longer queue behind writes: a dedicated
  read-only SQLite connection rides WAL beside the write mutex
  (`regent-store/infra/db.rs`; timing test proves a read completes during a
  held write transaction).
- **W2.1/W2.2 cron robustness** — `jobs.json` writes are atomic
  (tmp→rename, previous file kept as `.bak`); corrupt files recover from
  `.bak` (or empty+warn) instead of bricking cron; every load-mutate-save
  (5 admin ops + the tick's persist) serializes under a `.jobs.lock`, and the
  tick merges only the jobs it changed so a concurrent `cron add` survives.
- **W3.1 SSRF pin** — `guarded_get_bytes` connects to the exact validated IP
  (`reqwest .resolve()` per redirect hop), closing the DNS-rebinding TOCTOU.
- **W3.4 secrets ACL** — `.env` writes now set an owner-only Windows ACL
  (icacls) in both the Rust `manage_keys` tool and the CLI keys/setup writers
  (new `shared/infrastructure/storage/lockdown.ts`).
- **W1.2 ingress jail** — keyed (webhook/gateway) sessions always run in the
  filesystem sandbox; integration test proves an external turn's out-of-
  workspace `read_file` is rejected. (ADR-030)
- **W1.3 voice scope** — the voice deacon's auto-approver is now
  `VoiceScopedApprover`: terminal/computer_use/control_app mutations denied,
  benign actions approved; screen capture/vision unaffected (they're ungated
  reads). `REGENT_VOICE_FULL_CONTROL=1` restores blanket approval. The plan's
  Python target was already fixed; the live hole was the Rust voice server's
  defaults.
- **W2.3 memory gate wired** — external sessions' `memory add` stages into
  `pending_writes` (owner approves via existing memory.pending RPC);
  replace/remove refused there; local sessions unchanged. (ADR-030)
- Platform keys: `regent keys` + `manage_keys` now manage every platform
  token the gateway/webhooks actually read (Slack/WhatsApp/Messenger/LINE/
  Mattermost/Twilio/Teams/Feishu/WeChat/WeCom/Mailgun/Jira/Azure DevOps/
  Trello/GChat/speech + Telegram allowed-users, Discord public key).

**New capabilities:**
- **Camera vision on calls + CLI** — new `camera_capture` tool: during a
  `regent call` the call UI streams a JPEG frame every 2.5s (camera optional,
  audio-only fallback) to the token-gated `/call/frame` route → the tool
  returns the fresh frame for `vision_analyze` ("what am I holding?"); outside
  calls it falls back to ffmpeg webcam capture. Screen questions keep using
  computer_use screenshots.
- **Automatic coding harness** — new `code_task` tool in every chat session
  routes nontrivial code changes through plan→execute→verify→revert
  (ADR-027's flow, model-routed, re-entrancy-guarded).
- **Standalone build** — or-core/or-mcp vendored under
  `src/crates/regent-orchustr-core/`; no sibling Orchustr checkout needed.
  (ADR-032)
- **Token efficiency** — deferred toolsets: 12 rare tools' schemas withheld
  per request until `load_tools` fetches them; skills-index hooks capped at
  140 chars. No prompt/constitution changes. (ADR-031)
- **Local/offline models** — `regent setup` with provider `ollama` skips the
  API-key prompt and shows the live ollama model list (or install/pull hints);
  provider plumbing already spoke keyless OpenAI-compat to localhost:11434.
- **doc-forge** — bundled skill (compiled into the deacon, seeded into
  `$REGENT_HOME/skills` at boot, user edits never overwritten): designed
  pptx/docx/xlsx/PDF/CSV with runtime detection (python/bun lanes), a design
  system, and verification steps.

## 2026-07-02 — fix(voice/cron): agent connects reliably · fillers tamed · cron survives reboots

Three reported bugs:
1. **Calls echoed "I heard you say…" (agent never connected).** Root causes:
   the server only saw the API key/model when the CLI injected them (a
   manually started server had neither), and a failed boot-time spawn was
   never retried. Now `spawn_agent` backfills env from `$REGENT_HOME/.env` +
   config.yaml (`model.default`/`base_url`; real env wins), and every turn
   runs `ensure_agent`: dead deacons are detected (pipe-close flag) and
   respawned with a 30s cooldown. Echo mode also SAYS why ("my agent brain
   isn't connected — <reason>") instead of leaving the caller guessing.
   Verified live: scratch-home server reports `agent: ready`.
2. **"One moment, looking that up" on almost every reply.** The filler bridge
   fired at 1.6s, under the typical agent first-token time. Raised to 2.5s
   (quick replies now skip it) and the pool is 8 shorter, varied lines.
3. **Cron/scheduled jobs didn't fire (and died with reboots).** Jobs only
   tick inside a running deacon. New: `regent-deacon --keepalive` (serves the
   cron/board loops after stdin closes) + `regent cron autostart [--remove |
   --status]` — a Windows logon task (schtasks) that starts the keepalive
   deacon now and at every logon. The cron tick lock prevents double-firing
   next to session deacons; missed runs fire on the first tick back
   (lateness catch-up).
- Vision/computer-use on calls: already wired (voice deacon gets the full
  tool catalog, `REGENT_COMPUTER_USE=1` by default, screen via computer_use +
  vision_analyze). A camera-capture tool does NOT exist yet — screen yes,
  webcam no; flagged as follow-up.
- Files: `regent-voice-server` spawn.rs/deacon.rs/http.rs/main.rs/turn.rs +
  Cargo.toml (+serde_yaml); `regent-deacon` bin (keepalive); `regent-cli`
  cronCommand.ts + help.ts; `regent-agent` CAPABILITIES (cron autostart).
- Verified: voice-server 16 + deacon 49+26 + agent 28 tests green; clippy;
  CLI tsc/biome/38 tests; release binaries rebuilt; smoke: engines warm in
  ~12s from existing models, scratch-home agent ready, token/host gates hold.

## 2026-07-02 — feat(cli): `regent migrate hermes|openclaw` — import an existing install

`regent migrate <hermes|openclaw> [--home <path>] [--apply]` — dry-run by
default, additive only (source untouched, existing Regent skills never
overwritten).
- **Hermes:** skills import — Hermes's `skills/<category>/<skill>/SKILL.md`
  tree flattens into Regent's `skills/<name>/` (both agentskills.io format,
  so content copies as-is). Verified against the real `~/.hermes` here:
  82 skills detected in dry-run. Memories / state.db / cron / config are
  detected and reported as not-imported-yet (this machine's Hermes home has
  none of them — importers land when there's data to map).
- **OpenClaw:** source detection + honest "not implemented yet" listing (no
  OpenClaw install exists here to map against).
- Registered in router/help/command groups + the agent's CAPABILITIES
  (hand-to-user list — it edits files, no RPC).
- Files: `regent-cli` features/migrate/cli/migrateCommand.ts (new),
  router.ts, help.ts, commands.ts; `regent-agent` domain/prompts.rs.
- Verified: CLI `tsc` + biome + 38 tests green; live dry-run shows 82 skills
  → `~/.regent/skills`; `cargo test -p regent-agent` green.

## 2026-07-02 — fix(voice): live download progress in /health + the call UI

The first-run model download looked stuck: the engines note said "still
loading" with no hint a ~900MB fetch was running (and the old kokoro/piper/
Qwen3 files on disk are formats the sherpa engines can't read, so the fetch
is genuinely needed). Now `Engines::from_env_with(progress)` streams status
("downloading sherpa-onnx-whisper-small — 250/610 MB" → "unpacking…" →
"loading local engines…") into the /health note and the call turn's error
line. Extraction also became crash-safe: unpack into a temp dir, then rename
into place — a killed process can't leave a half-extracted folder the probe
would mistake for an install.
- Files: `regent-voice-server` infra/download.rs, infra/engines.rs, main.rs.
- Verified: `cargo test -p regent-voice-server` 16 green; clippy + fmt clean.

## 2026-07-02 — feat(voice): local ONNX engines — `regent call` runs on Rust now

The voice server gains real inference (user mandate: Rust + ONNX is the call
path now): whisper ASR + Kokoro TTS via sherpa-onnx, default-on.
- **engines (`local-onnx`, default feature):** `WhisperAsr` (sherpa offline
  recognizer, int8 preferred, `REGENT_WHISPER_SIZE`/`_DIR`/`_LANG`) +
  `KokoroEngine` (kokoro-en-v0_19, `REGENT_KOKORO_DIR`/`_SPEAKER`). Engines
  load in the background at boot (server is reachable instantly; /health
  flips `warm`); first run auto-downloads the sherpa bundles into
  `REGENT_MODELS_DIR` (skip with `REGENT_VOICE_AUTODOWNLOAD=0`) — parity with
  the Python server's download-on-first-run.
- **domain:** `wav::parse_pcm16_mono` — chunk-walking RIFF reader for ASR
  input (16-bit mono PCM; OGG/Opus voice notes still need an opus slice).
- **build:** sherpa-rs-sys runs bindgen (neither the crates.io package nor
  its git tag ships pregenerated bindings) → LLVM installed (winget) and
  `.cargo/config.toml` sets `LIBCLANG_PATH`. Prebuilt sherpa-onnx libs come
  via the crate's `download-binaries` default — no cmake in the loop.
- **CLI:** `regent voice serve` and `regent call` prefer the Rust binary
  (`REGENT_VOICE_SERVER_PATH` override → target/{release,debug} walk-up →
  PATH-side lookup); the Python server + its dep preflight are the fallback.
- Files: `regent-voice-server` infra/{sherpa,download}.rs (new) + engines.rs
  + http.rs + main.rs (engines behind RwLock, background load) +
  domain/wav.rs (new) + Cargo.toml; workspace Cargo.toml (+sherpa-rs/tar/
  bzip2); `.cargo/config.toml` (new); `regent-cli` voiceServe.ts +
  callServe.ts.
- Verified: `cargo test -p regent-voice-server` 16 green (WAV round-trip,
  int8 probe preference); release binary builds + live smoke test (health
  notes, /call/turn 401 without token, 403 on a non-local Host); CLI `tsc` +
  biome + 38 tests green. Full inference check rides the first real call's
  model download.

## 2026-07-02 — feat(voice): regent-voice-server — Rust port of the speech server (secured)

New `regent-voice-server` crate: the python-voice-server's HTTP surface,
call orchestration, and agent brain in Rust (ADR-029). Local ONNX engines are
the next slice — until then `/v1/audio/*` answer 503 with a clear note and
Python remains the speech path.
- **domain (pure, tested):** `strip_markdown`/`strip_spoken` (both Python
  `_speakable`s), `SentenceSplitter` (per-sentence TTS streaming; decimals
  don't split), `classify` (JSON-RPC line router, Python self-checks ported).
- **infra:** `DeaconRpc` — stdio JSON-RPC client into regent-deacon
  (demux by id, latest-wins `turn.interrupt`, drain; tested over an in-memory
  duplex with a scripted deacon) + `spawn_agent` (voice env contract:
  auto-approve/`REGENT_VOICE`/computer-use, kill-on-drop).
- **application:** `run_turn` — ASR → agent stream → per-sentence TTS as
  NDJSON (`heard`/`reply`/`audio`/`timing`), 1.6s filler bridge, echo fallback
  with no deacon. (Deliberately dropped: the raw completions fallback brain
  and librosa time-stretch — deacon-first; say so if either is needed.)
- **SECURITY (user-mandated), vs the Python server:** loopback bind + Host
  allowlist (DNS-rebinding guard); NO wildcard CORS — only the regent-web
  origin (:3000, + `REGENT_CALL_UI_ORIGIN`); `/call/turn` (full agent,
  auto-approved tools) gated by a per-boot token (embedded in /call, served
  to allowed origins at `/call/token`); UI assets compiled in (no traversal);
  25 MB / 8k-char body caps. 5 security tests (tower oneshot).
  The Python server got the interim fixes too: origin allowlist replaces
  `allow_origins=["*"]` + an Origin check on `/call/turn` (no-cors CSRF).
- **regent-web:** `useCall.ts` split to honor the 200-line rule —
  `localCall.ts` (VAD turn loop) + `speechServer.ts` (URL/token/WAV/playback);
  turns now send `x-call-token` (fetched once; "" against the Python server).
- Files: `src/crates/regent-voice-server/**` (new) + workspace member;
  `python-voice-server/{python_server.py,web_call.py,ui/call.html}`;
  `src/regent-web/hooks/{useCall,localCall,speechServer}.ts`.
- Verified: `cargo test -p regent-voice-server` 13 green; clippy + rustfmt
  clean; `py_compile` clean; regent-web `tsc` clean.

## 2026-07-02 — feat(constitution): vectorize — core in the prompt, full document in tri-modal memory

Phase 2 of ADR-028: the always-on constitution shrinks to a token-efficient
CORE while the full document becomes retrievable memory (Graph + SQLite/FTS5 +
Vector + rank-fusion, per ADR-013).
- **domain (pure, tested):** `constitution_sections()` (parse the 16 `## N.`
  sections), `constitution_core(name)` (preamble + §3 Character + the
  safety-relevant §11/§12/§14/§16 verbatim + an index pointing the agent at
  `memory_search` for the rest — limits never depend on retrieval recall),
  `constitution_chunks()` (per-section graph entries ≤ the 2,000-char cap;
  long bullet lists split per line; each chunk carries a `[Constitution §N —
  Title]` prefix so it stands alone when recalled).
- **application:** `regent-deacon` `sync_constitution` use case — enabled:
  seed/upgrade the persona row to the core (user edits untouched) + ingest
  chunks as pinned `constitution` nodes (UserStated trust, hash-dedup
  idempotent, stale-node reconcile on document updates); disabled: clear the
  shipped row + remove the nodes. Called from the composition root.
- `regent-graph`: `constitution` added to the dedup-lookup kinds. Retrieval
  needs no change — FTS/vector/graph fusion has no kind filter, and
  `render_prompt_block` (memory/user kinds only) stays unpolluted.
- Files: `regent-agent` domain/prompts.rs + lib.rs; `regent-deacon`
  application/constitution.rs (new) + mod.rs + lib.rs + bin; `regent-graph`
  application/orchestrators.rs.
- Verified: `cargo test -p regent-agent -p regent-graph -p regent-deacon`
  green (7 new tests: sections/core/chunks + 4 sync cases); clippy clean;
  touched files rustfmt-clean (pre-existing drift left alone).

## 2026-07-02 — feat(prompts): separate SYSTEM_PROMPT · CONSTITUTIONAL_PROMPT · CAPABILITIES (opt-in constitution)

The monolithic prompt in `regent-agent` is now three named layers in a pure
domain module, plus a new opt-in constitutional values layer (character +
hard boundaries, grounded in Christian biblical values).
- **domain:** `regent-agent/src/domain/prompts.rs` — `SYSTEM_PROMPT` (renamed
  from `BASE_PROMPT`, text unchanged), `CAPABILITIES` (unchanged), and
  `CONSTITUTIONAL_PROMPT` — a versioned document at
  `regent-agent/prompts/constitution.md` (16 numbered sections so its internal
  references resolve), `include_str!`'d; `constitution_text(name)` fills the
  `[Agent Name]` placeholder.
- **store:** `constitution` is a first-class persona row (valid key, seeded
  empty). `persona_block()` renders it FIRST with a supremacy header — both
  the deacon and the gateway inherit it with no prompt-assembly changes.
- **deacon:** additive `constitution.enabled` config (off by default). Boot
  sync: enabled + empty row → seed from the shipped document; disabled →
  clear only an unmodified copy (a user-edited constitution is kept).
- **CLI:** `regent setup` gains a Constitution opt-in question (+
  `--constitution y|n` for non-interactive runs), written to config.yaml.
- Files: `regent-agent` prompts.rs + prompts/constitution.md (new), lib.rs,
  domain/mod.rs; `regent-store` infra/persona.rs; `regent-deacon`
  domain/config.rs + bin/regent-deacon.rs + session_manager/build.rs (rename);
  `regent-gateway` bin/gateway.rs (rename); `regent-cli` setupCommand.ts.
- Decisions: ADR-028. Next (same task): vectorize — ingest the sections into
  graph memory (tri-modal retrieval) and shrink the always-on block to a core.
- Verified: `cargo test -p regent-agent -p regent-store -p regent-deacon`
  green (5 new tests); clippy clean; touched files rustfmt-clean; CLI `tsc`
  clean + 38 `bun test` green; setupCommand's one biome hit predates this.

## 2026-07-01 — feat(regent-code): §F P1 coding harness — plan-mode → verify → revert

The flagship of §F: a coding-specialized harness over `regent-agent` that does
**plan-mode gate → edit → per-step verify → revert-to-last-green**, surfaced as
`regent code`. A disciplined wrapper around the existing loop, not a rewrite.
- **domain (pure, unit-tested):** `detect_build_tool` (repo root manifests →
  Cargo > Npm > Pytest > Make), `BuildTool::verify_command`, `plan_toolset`
  (read-only subset `read_file`/`glob`/`search_files`/`ls` in plan mode),
  `parse_verify` (exit + stdout/stderr → `{passed, summary}`).
- **application:** `CodeHarness` loop over `Agent` — plan (read-only) → approve →
  execute (full toolset) → verify → revert-on-fail. Phase prompts adopt Claude
  Code's plan-mode discipline (read-only supersedes; explore + reuse; structured
  Context/Approach/Files/Reuse/Verification plan; execute = root-cause, no gold-plating).
- **infra:** `VerifyRunner` (spawn the detected test/build lane) + `GitCheckpoint`
  (snapshot before execute; on fail restore tracked files + remove new ones;
  outside git → report-only). `Verifier`/`Checkpoint` are ports so the loop is
  testable without real builds.
- **surface (§H):** `code.plan` (read-only → PLAN) + `code.start` (snapshot →
  execute → verify → revert) RPC, run over the daemon's existing session path so
  approval/streaming/interrupt are reused (ADR-027 approach A); `regent code
  "<task>" [--yes]` CLI with a y/N plan gate; CAPABILITIES + help + command groups
  gain `code`. Both RPC methods are on the admin-tool DENY list (no self-driving).
- Plan-mode read-only is enforced structurally (`ToolCatalog::restrict_to`), not by
  prompt — write/terminal tools are absent from the plan turn's catalog.
- Files: `src/crates/regent-code/**` (new crate); `regent-deacon`
  `session_manager/{code,lifecycle,admin}.rs` (split out — mod.rs back to 178 lines)
  + `dispatcher/code_ops.rs`; `regent-agent` CAPABILITIES; `regent-cli`
  `features/code/cli/codeCommand.ts` + router/help/commands.
- Decisions: ADR-027. P2 (deferred): auto-routing coding chat, worktree isolation,
  code-context RAG — on the proven core, not before.
- Verified: `cargo test` — regent-code 14 (domain + git checkpoint + harness_flow
  integration over a scripted provider), regent-deacon 44+26; clippy + fmt clean.
  CLI `tsc` + `biome` + command/args tests green.

## 2026-07-01 — refactor(deacon)!: rename regent-daemon → regent-deacon

User-requested full rename of the core daemon crate. Renamed the crate dir +
package + `regent-deacon` binary + bin file, the integration test file
(`deacon_basics.rs`), every `regent_daemon` import/path, the CLI + Python
daemon-spawn paths, and the `REGENT_DAEMON_PATH` → `REGENT_DEACON_PATH` override.
- **BREAKING:** the binary is now `regent-deacon`; build with `cargo build -p
  regent-deacon`; the path-override env var is `REGENT_DEACON_PATH`.
- Extended (same request) to the daemon-named identifiers: `DaemonConfig`→`DeaconConfig`,
  `DaemonError`→`DeaconError`; the TS `shared/infrastructure/daemon/` dir → `deacon/` with
  `locateDeacon`/`connectDeacon` + the `deacon-locate`/`deacon-spawn` failure codes.
- Intentionally unchanged: the generic word "daemon" (the OS process concept, in prose) and
  the `Dispatcher` type — not the `regent-daemon`/`Daemon*` token. Historical changelog
  entries below keep their original wording (a dated record).
- Verified: regent-deacon builds + 44+26 tests green; `regent-deacon.exe` builds;
  CLI `tsc` clean; renamed files `biome`-clean.

## 2026-06-30 — feat(tools): ls — list a directory (§F coding triad)

- `ls` tool: list a directory's immediate entries (name · dir|file · size),
  dirs first. Non-recursive — completes the Claude-Code coding triad with `glob`
  (path patterns) and `search_files` (content grep). Jailed via `ToolContext`.
  Registered in core. The §F editing craft (file_edit · glob · grep · apply_patch
  · ls) is now in place as tools; the `regent-code` *crate* (plan-mode gate +
  per-step verify + worktree) remains the larger harness piece.
- Files: `regent-tools/infra/ls.rs` (new), `infra/mod.rs`, `application/registry.rs`.
- Verified: `cargo test -p regent-tools` green; clippy clean.

## 2026-06-30 — fix(voice/cli): TTS symbols · noise · welcome art · call vision/computer-use

Four reported bugs:
1. **TTS read symbols aloud** ("asterisk", "slash", …). Added `_speakable()` to
   the voice server — strips markdown/structural symbols (`*_~#>|`, backticks,
   `/`, bullets, headings, numbered lists, `[label](url)`→label) before synthesis;
   applied in both Kokoro + Piper TTS wrappers (one chokepoint).
2. **Welcome art dwarfed by long left column.** `WelcomePanel` now caps category
   rows (6/section) and items per line (6) and collapses overflow to "…" — the
   left text stays compact so the king mark keeps its size. (Also fixed a
   pre-existing `??=`-in-expression lint in `groupBy`.)
3. **Background noise transcribed as words.** Enabled faster-whisper `vad_filter`
   (+`min_silence_duration_ms: 300`) so non-speech/silence is dropped before decode.
4. **Calls couldn't see the screen / drive apps.** The call routes through the
   full agent daemon (`web_call.py`), which now also enables `computer_use`
   (`REGENT_COMPUTER_USE=1`) by default for voice — so "look at my screen / open
   this site" works: screenshot→see→click/type, plus vision_analyze (always in
   catalog) and browser control (when `REGENT_BROWSER_MCP_URL` is set). Opt out
   with `REGENT_VOICE_COMPUTER_USE=0`. (Needs the rebuilt daemon + a vision key.)
- Files: `python-voice-server/python_server.py`, `python-voice-server/web_call.py`,
  `regent-cli/app/presentation/WelcomePanel.tsx`.
- Verified: `tsc` + `biome` clean; `py_compile` clean; `_speakable` spot-checked.

## 2026-06-30 — feat(tools): video_analyze — analyze a video → text (§C)

- `video_analyze` tool (Hermes `video_analyze` gap): analyze a video (http(s)
  URL / local path / data: URL, ≤50 MB) and return a description/answer. Mirrors
  `vision_analyze` — reuses the shared SSRF guard + the vision provider config,
  with an optional `REGENT_VIDEO_MODEL` override; sends a `video_url` content
  part to a video-capable model. Registered in core (`media` toolset).
- Files: `regent-tools/infra/video_analyze.rs` (new), `infra/mod.rs`,
  `application/registry.rs`. Verified: `cargo test -p regent-tools --lib
  video_analyze` 2/2; clippy clean.

## 2026-06-30 — perf(call): tighten voice-call startup polling 500ms→250ms

- `regent call` polled the speech backend (server-up + models-warm) and the web
  UI readiness every 500ms; readiness usually lands mid-tick, so the call could
  wait up to ~0.5s per gate after the service was actually ready (≈1s across the
  up + warm + browser-open gates). Tightened to 250ms ticks (loop bounds doubled
  to keep the same ~30s budgets), so the call connects as soon as it's ready —
  shaving ~0.75–1s off perceived start. Lightweight localhost polls; no new deps.
- Note: chat *response* latency has no fixed delay to cut — the daemon persists
  across the chat (spawned once), deltas flush at 50ms, and the provider retry
  backoff sleeps only on failure; remaining latency is the model API + the
  one-time session/prompt build.
- Files: `regent-cli/features/call/cli/callServe.ts`. Verified: `tsc` + `biome` clean.

## 2026-06-30 — feat(tools): full tools.list catalog + CAPABILITIES lists all tools

- `tools.list` (and the welcome panel's **Tools** section + `regent tools list`)
  now returns the **full session catalog** — core + memory + skills + kanban +
  persona + keys + delegate + message + the in-process `regent` tool + (opt-in)
  browser + the new file_edit/apply_patch/glob/vision_analyze/image_generation/
  computer_use — instead of just the bare core set. Extracted
  `SessionManager::build_main_catalog` (shared by session build + listing, so the
  panel and the agent never drift) + `list_tool_definitions`; `tools.list` is now
  async over it. The per-surface `disable` filter + RPC hook stay session-only.
- `CAPABILITIES` prompt now enumerates the agent's actual tool abilities (find
  files, precise edits, see/generate images, drive the desktop via computer_use,
  …) so the agent knows what it can do.
- Files: `regent-daemon` (`session_manager/build.rs`, `dispatcher/admin_ops.rs`,
  `dispatcher/mod.rs`), `regent-agent/lib.rs`.
- Verified: `cargo check -p regent-daemon -p regent-agent` clean; daemon clippy clean.

## 2026-06-30 — feat(tools): image_generation — text→image (§C)

- `image_generation` tool (Hermes `image_generation_tool` gap): generate an image
  from a prompt via an OpenAI-compatible `/images/generations` endpoint
  (`b64_json`), save the PNG under the artifacts dir, reveal it, return the path.
  Self-contained + env-config (`REGENT_IMAGE_BASE_URL`/`REGENT_IMAGE_MODEL`/
  `REGENT_IMAGE_API_KEY`, falls back to `REGENT_API_KEY`); registered in core.
- Files: `regent-tools/infra/image_generation.rs` (new), `infra/mod.rs`,
  `application/registry.rs`. Verified: `cargo test -p regent-tools --lib
  image_generation` 1/1; clippy clean.

## 2026-06-30 — feat(tools): glob — find files by path pattern (§C / §F)

- `glob` tool: find files by glob (`**/*.rs`, `src/**/test_*.py`) — the
  Claude-Code complement to `search_files` (which is content `grep`). Pure
  `glob_to_regex` translator (`**/`→optional dir prefix, `*`→within-segment,
  `?`→one char; regex metachars escaped), unit-tested; walks the tree skipping
  `.git`/`target`/`node_modules`/etc. No new deps (reuses `regex`+`walkdir`).
- Files: `regent-tools/infra/glob.rs` (new), `infra/mod.rs`, `application/registry.rs`.
- Verified: `cargo test -p regent-tools --lib glob` 2/2; clippy clean.

## 2026-06-30 — feat(tools): apply_patch (V4A multi-file) + computer_use as default GUI automation (§C)

- `apply_patch` tool (Hermes `patch_parser` / V4A-style diffs): apply a
  `*** Begin Patch` … `*** End Patch` envelope with `Add`/`Update`/`Delete File`
  sections in one call. Pure V4A parser (`parser.rs`, unit-tested) split from the
  applier; Update hunks apply as anchored replaces (context+removed block must
  match uniquely, like `file_edit` but multi-line/multi-file). Registered in core.
- `computer_use` description now marks it the **preferred GUI-automation path**
  (browser, desktop apps, typing, clicking) whenever a direct API/CLI isn't
  practical; fixed the module doc (CUA is the default backend, not PowerShell).
- Files: `regent-tools/infra/apply_patch/{mod,parser}.rs` (new, each ≤200 lines),
  `infra/mod.rs`, `application/registry.rs`, `infra/computer_use/mod.rs`.
- Verified: `cargo test -p regent-tools --lib apply_patch computer_use` green
  (apply_patch 4, computer_use 5); clippy clean.

## 2026-06-30 — feat(tools): computer_use — desktop control via CUA (§C)

- `computer_use` tool (Hermes `computer_use` gap): coordinate-based desktop
  control — `screenshot` · `click(x,y)` · `type(text)` · `key(combo)`. The model
  drives a screenshot→read(vision_analyze)→act loop; the tool runs one action
  per call. **High-privilege**, so: feature-flagged (only registered when
  `REGENT_COMPUTER_USE=1`), every mutating action approval-gated (screenshot is
  read-only), screen content treated as untrusted data (§10.2).
- **Default backend is CUA** (`CuaBackend`) — drives the cross-platform
  `cua-driver` binary (trycua/cua), the same driver Hermes uses, via
  `cua-driver call <tool>` (screenshot/click/type_text/hotkey). Binary
  configurable via `REGENT_CUA_DRIVER_CMD`; missing binary → install hint.
- Fallback `PowerShellBackend` (native Windows: `System.Drawing` capture +
  user32 P/Invoke, no new native deps) selectable via
  `REGENT_COMPUTER_USE_BACKEND=powershell`. Backends behind the `ComputerBackend`
  trait — tests use a mock (no real input injection).
- Files: `regent-tools/infra/computer_use/{mod,cua,powershell,tests}.rs` (new,
  each ≤200 lines), `infra/mod.rs`, `application/registry.rs`.
- Verified: `cargo test -p regent-tools --lib computer_use` 5/5 (parse · feature+
  approval gating · sendkeys/combos · cua image extraction); clippy clean.
  Note: the `cua-driver call` JSON contract follows cua-driver-rs docs — validate
  against an installed cua-driver (not runnable in this environment).

## 2026-06-30 — feat(tools): vision_analyze + shared SSRF guard (§C vision)

- `vision_analyze` tool (Hermes `vision_tools.py` port, text path): analyze an
  image and return a description/answer. Accepts an `http(s)` URL, a local file
  path (jailed), or a `data:` URL; sniffs mime from magic bytes; base64-encodes
  and sends to a vision model over an OpenAI-compatible endpoint; returns text.
- Regent's chat contract is text-only, so the tool **owns its own vision call**
  and returns text — no shared-contract/wire-adapter surgery. Vision model
  configured by env (mirrors web_search): `REGENT_VISION_BASE_URL` ·
  `REGENT_VISION_MODEL` (default `google/gemini-2.5-flash`) · `REGENT_VISION_API_KEY`
  (falls back to `REGENT_API_KEY`). 20 MB cap.
- Extracted the SSRF guard into `infra/net.rs` (`guarded_get_bytes` +
  `is_blocked_ip`/`validate_public_url`) — **one** implementation now shared by
  `web_fetch` and `vision_analyze`, so the private-IP denylist can't diverge.
  `web_fetch` refactored to use it (behavior unchanged; SSRF tests moved to net).
- Files: `regent-tools` (`infra/vision_analyze.rs` new, `infra/net.rs` new,
  `infra/web_search.rs` refactor, `mod.rs`, `application/registry.rs`, +`base64` dep).
- Verified: `cargo test -p regent-tools` 75/75 (incl. web_fetch refactor, vision 4,
  net 2); clippy clean (pre-existing search_providers warning untouched).

## 2026-06-30 — feat(tools): file_edit — anchored unique string-replace (§C file-ops / §F.1)

- `file_edit` tool: replace an exact, UNIQUE `old_string` with `new_string`;
  fails if absent (NotFound) or non-unique (Ambiguous, with match count). Claude
  Code's FileEdit contract — the biggest editing win over whole-file `write_file`
  (change one spot without clobbering the rest), and the core primitive §F
  (regent-code) builds on.
- Pure core `apply_anchored_edit(src, old, new)` (unit-tested) split from the
  executor (path-jail I/O via `ToolContext::resolve`). Empty/identical
  `old_string` rejected; an ambiguous match leaves the file untouched.
- Registered in the core catalog beside read_file/write_file; auto-appears in
  `regent tools list` (model-facing tool, not a CLI command — no §H sync needed).
- Files: `regent-tools/infra/file_edit.rs` (new), `infra/mod.rs`, `application/registry.rs`.
- Verified: `cargo test -p regent-tools --lib file_edit` 6/6; clippy clean (the
  pre-existing search_providers "items after test module" warning is untouched).

## 2026-06-30 — feat(mom): MoM surface — config groups + `mom.run` RPC + `regent agents mom` CLI (§B.P1)

- Makes MoM runnable end-to-end. `config.mom` is a named-group map
  (`name → {proposers: [specs], aggregator, max_proposers}`); specs are
  `"provider/model"` (or bare → `agents_defaults.primary`) resolved through the
  provider registry (item A). Additive, `deny_unknown_fields`.
- `mom.run` RPC: resolves a group's proposer + aggregator specs, fans out, returns
  the aggregator's synthesis. Unresolvable proposers are skipped (logged); an
  unresolvable aggregator is a hard error. Resolution is a sync `prepare_mom`
  helper so no `self`/config borrow crosses the await (keeps the in-process
  `regent` tool path `Send`).
- `MomRunner::run` now builds owned proposer futures in a for-loop + `join_all`
  (concurrency bounded by `max_proposers`) instead of a lazy `buffered` stream —
  the lazy `.map` closure borrowed `&self` across the await and tripped the
  nested-async_trait `Send` HRTB.
- CLI `regent agents mom run|create|list|remove` (in-chat `/agents mom …`):
  `run` calls `mom.run`; `create/remove` edit `config.yaml`'s `mom` map (mirror
  `providers`/`tools`); `list` reads it. §H four-source sync updated (CAPABILITIES,
  regent_tool method list, help, router) — agent can run `mom.run` itself.
- Files: `regent-daemon` (`domain/config.rs` +group struct/test, `dispatcher/admin_ops.rs`
  +`mom_run`/`prepare_mom`, `dispatcher/mod.rs` route, `regent_tool.rs`, `tests/daemon_basics.rs`
  +1 test), `regent-agent` (`mom/mod.rs` run() refactor, `lib.rs` CAPABILITIES),
  `regent-cli` (new `agents/cli/momCommand.ts`, `router.ts`, `help.ts`).
- Verified: `cargo test -p regent-agent -p regent-daemon` green (mom 5/5, +1 dispatcher
  test); clippy clean; CLI `tsc` + `biome` clean.

## 2026-06-30 — feat(mom): Mixture-of-Models runner (§B.P1 core)

- `MomRunner` (regent-agent `application/mom/`): N **proposer** models answer a
  brief in parallel (advisory — no tools, no agent loop), an **aggregator** model
  synthesizes their answers into one. The Mixture-of-Agents technique (Together's
  paper; mirrors Hermes `moa_loop.py`) with *model-level* proposers — named MoM,
  not MoA, because the units are models, not full agents.
- Proposers run through the same bounded, order-preserving fan-out `delegate_task`
  uses (`futures::buffered`). A failing/empty proposer is **dropped, not fatal** —
  the aggregator synthesizes from survivors; zero survivors ⇒ aggregator answers
  the brief alone. `max_proposers` caps cost (default 3).
- Takes **pre-resolved** `ChatProvider`s (the daemon resolves `ModelRef`s through
  item A's registry), so `regent-agent` stays free of provider-config types — and
  each proposer can be a different model (the point of MoM).
- Files: `regent-agent/application/mom/mod.rs` (+exports). Pure `aggregator_brief`
  unit-tested; 3 scripted-provider tests (aggregator sees all proposals · failing
  proposer skipped · max_proposers caps). Surface (config + `mom.run` RPC + CLI) next.
- Verified: `cargo test -p regent-agent --lib mom` 5/5; clippy clean.

## 2026-06-30 — feat(providers): `regent providers` CLI + `providers.*` RPC (§H)

- Finishes item A's user-facing surface: `regent providers list | add | remove | test`
  (and `/providers` in chat). Per the §H no-drift rule, the command lands in all
  four sources at once — `commands.ts`, `router.ts`, `help.ts`, and the agent
  `CAPABILITIES` — plus the in-process `regent` tool's method list.
- `providers.list` (RPC) — configured providers with `key_present` (whether the
  `api_key_env` is set; never the key itself). `providers.test <name|provider/model>`
  (RPC) — resolves via the registry and sends a tiny live completion to confirm
  the key + endpoint work; returns `{ok, model, error?}`.
- `providers add/remove` edit `config.yaml`'s `providers` map directly (atomic
  tmp+rename), mirroring `tools enable/disable` — no mutation RPC; the daemon
  reloads config next run. `add` validates `--kind` against the ProviderKind set
  (actionable error, not a stack trace) and requires `--key-env` + `--models`.
- In-process parity: the agent can run `providers.list`/`providers.test` itself;
  `add/remove` are flagged as config-edit commands it hands to the user.
- Files: `regent-daemon` (`dispatcher/admin_ops.rs` +2 handlers, `dispatcher/mod.rs`
  routes, `regent_tool.rs` method list, `tests/daemon_basics.rs` +2 tests),
  `regent-agent/lib.rs` (CAPABILITIES), `regent-cli` (new `features/providers/cli/`,
  `commands.ts`, `router.ts`, `help.ts`).
- Verified: `cargo test -p regent-daemon -p regent-agent` green (+3 dispatcher tests);
  CLI `tsc --noEmit` + `biome check` clean + `bun test` 38/0.

## 2026-06-30 — feat(providers): per-agent model/provider at the board runner (§A.P2, ADR-026)

- Closes the documented gap (`board/runner.rs`: *"a `model` override is stored on
  the agent but applied at the provider layer — not here yet"*). A named agent
  with a `model` override now actually runs on that model's provider.
- `AgentTaskRunner` gains an optional `ProviderResolver` (`Fn(&str) -> Option<Arc<dyn ChatProvider>>`)
  via `with_resolver`; `resolve()` returns the per-agent provider. Absent/unresolved
  ⇒ the shared default provider — a task is never blocked on model config.
- The daemon builds the resolver over `ProviderRegistry` + `agents_defaults`
  (primary + fallback chain). `regent-agent` stays free of provider-config types
  (the resolver is an opaque closure) — clean dependency direction.
- Empty `providers` map ⇒ resolver no-ops ⇒ identical to today's behavior.
- Files: `regent-agent` (`board/runner.rs` +`with_resolver`/resolver field/2 new
  tests, `board/mod.rs`, `lib.rs` exports), `regent-daemon`
  (`board_dispatch.rs` builds the resolver, `bin/regent-daemon.rs` wires the registry).
- Verified: `cargo test --workspace` fully green; `cargo clippy` clean on the three
  touched crates; runner tests 5/5. (Pre-existing fmt/clippy debt in untouched
  `regent-cron`/`regent-tools` left alone — not part of this change.)

## 2026-06-30 — feat(providers): multi-provider registry + config + model.list merge (§A.P1, ADR-026)

- First atomic step of next-batch §A. **Additive only** — empty config = today's
  single-provider behavior; the prompt-cache freeze is untouched.
- `ModelRef { provider, model }` — new kernel value type (the only shared piece);
  provider-aware `"provider/model"` parsing lives in the registry, not the type.
- `config.providers` map (`name → {kind, base_url, api_key_env, models}`) +
  `agents_defaults {primary, fallbacks}`. `deny_unknown_fields` honored; one
  `api_key_env` serves every model in `models` (multi-model-per-key, §3).
- `ProviderRegistry` (in `regent-daemon`, reusing `make_provider_factory` +
  `FallbackChat`): resolves+memoizes `ModelRef → provider`, builds per-agent
  fallback chains, typed `RegistryError` (UnknownProvider/MissingKey). Keys read
  from env at resolve time, never stored.
- `model.list` merges configured providers' models as `"<provider>/<model>"`
  (sorted, stable) so the id round-trips through `model.set`.
- Files: `regent-kernel/types/model_ref.rs` (+exports), `regent-daemon/domain/config.rs`,
  `regent-daemon/application/provider_registry.rs` (new), `dispatcher/admin_ops.rs`,
  `infra/config_loader.rs` (incidental: collapsed two let-chains for clippy).
- Verified: `cargo test -p regent-kernel -p regent-daemon` (8 new tests green),
  `cargo clippy` clean. ADR-026. Per-agent wiring at the board runner is §A.P2 (next).

## 2026-06-30 — docs(proposal): full next-batch implementation plan

- Detailed, build-ready plan for the TO-ADD batch (plan-only, per the task):
  [`docs/proposal/regent-next-batch-full-plan-v1.md`](proposal/regent-next-batch-full-plan-v1.md),
  companion to the overview (`regent-next-batch-v1.md`). Written through four
  lenses — **Rust-engineer** (types/traits/errors/validation gates), **RAG-architect**
  (skills-as-RAG + code-context retrieval with eval thresholds), **ML-pipeline**
  (evals/golden-sets, model tiering, reproducibility), **CLI-developer** (§H command
  surface, single-source-of-truth rule so help/`/`-menu/`CAPABILITIES`/in-process
  tool never drift).
- Covers: A multi-provider + per-agent model/fallback + multi-model-per-key ·
  B Mixture-of-Agents · C Hermes tools (vision→computer-use→file-ops→…) · D Hermes
  skills (ports + retrieval) · E code-context RAG · F `regent-code` crate · G
  real-time calls · plus a cross-cutting evals harness. Each phase is gated behind
  its own ADR (next free: ADR-026), additive-only, with tests in the same change.
  Nothing built yet — awaiting the user's open-decision answers.

## 2026-06-30 — fix(tools): play picks the *actual* song (rank top 5, prefer official)

- `play` took yt-dlp's #1 result blindly, which is often a lyric video, cover, or
  live cut — so it played the wrong thing. It now searches the top 5 and ranks
  them (`pick_best`): prefer the official upload (title "official", or a VEVO /
  "- Topic" / official channel) weighted by view count, and down-rank
  live/cover/lyric/remix/etc. cuts the user didn't ask for. If the query itself
  names a variant ("… live", "… acoustic"), only those are kept (intent beats
  popularity). Ranking is dynamic per request; unit tests use canned rows.
- Files: `regent-tools/infra/play.rs` (+ tests). Verified: `cargo test
  -p regent-tools play` 3/3, clippy clean. Needs a daemon rebuild to take effect.

## 2026-06-30 — feat(gateway): send_file for Messenger, Feishu, WeCom, Mattermost (Bug #3)

- Gateway file delivery (`send_file`) previously worked on only 5 platforms
  (telegram/discord/slack/whatsapp/wechat). Added `WebhookFileSender` for **four
  more** byte-uploadable platforms, each registered in `file_senders_from_env`
  (gated on the platform's outbound creds):
  - **Mattermost** — upload `/api/v4/files` → post with `file_ids`.
  - **Messenger** — multipart Send-API attachment (`filedata`) + caption follow-up.
  - **Feishu** — `im/v1/files` → `file` message (tenant token).
  - **WeCom** — `media/upload?type=file` → `file` message (access token + agentid).
- Not done by design (can't upload local bytes): **LINE / Twilio SMS** are
  URL-only (media must already be hosted), and **Teams / Google Chat** have no
  outbound credential in their adapters — these decline `send_file` as before.
- Files: `regent-gateway/infra/platforms/{mattermost,messenger,feishu,wecom}.rs`,
  `regent-daemon/infra/webhook.rs`. Verified: `cargo build -p regent-daemon` +
  `cargo clippy -p regent-gateway` clean (request-building only; no live creds to
  test against). Needs a daemon rebuild to take effect.

## 2026-06-30 — fix: play resolves yt-dlp off-PATH; CLI outlines preambles too

- **`play` fell back to a search** (and the agent did a slow web workaround → big
  delay) because yt-dlp is a pip user-install whose Scripts dir isn't on the
  daemon's PATH, and `py`/`python` pointed at an interpreter without the module.
  `play` now **discovers** `yt-dlp` in the common pip Scripts locations
  (`%LOCALAPPDATA%`/`%APPDATA%\Python\<tag>\Scripts\yt-dlp.exe`; `~/.local/bin`
  etc. elsewhere) and calls it by absolute path, so it resolves the canonical
  watch URL directly instead of searching.
- **Mid-turn preambles now get the box outline** too: every non-empty assistant
  message (preamble *and* final reply) is framed; the rendered-content guard
  still suppresses empty/think-only artifacts, so no empty boxes.
- Files: `regent-tools/infra/play.rs`,
  `regent-cli/.../components/TranscriptItem.tsx`. Verified: `cargo clippy
  -p regent-tools` clean, `tsc`/biome clean, `bun test` 38/38. **Rebuild the
  daemon + recompile the CLI** to take effect (close `regent` first — it locks
  both binaries).

## 2026-06-30 — fix(tools): the `play` tool can't hang the turn anymore

- "Pull up <song>" sometimes left Regent **stuck on "thinking…"**: `play` shells
  out to `yt-dlp` to resolve the top result, with **no timeout** — a stalled or
  throttled yt-dlp blocked the whole turn indefinitely.
- Fix: each resolve is capped (15s, `kill_on_drop`); on timeout we stop and fall
  back to opening a YouTube search instead of hanging, and a not-installed
  invocation still skips quickly to the next. Bounds the worst case to ~15s.
- Files: `regent-tools/infra/play.rs`. Verified: `cargo clippy -p regent-tools`
  clean. Needs a daemon rebuild to take effect.

## 2026-06-30 — fix(cli): show mid-turn preambles with a dim "✦ Regent" label

- After hiding empty boxes, mid-turn preambles (Regent's "on it, searching…"
  before a tool) rendered as bare, easy-to-miss text. They now show under a dim
  `✦ Regent` label (no box) so the acknowledgment is visible and attributed,
  while the final reply keeps its full box. Empty/think-only artifacts still
  render nothing.
- Files: `regent-cli/features/chat/presentation/components/TranscriptItem.tsx`.
  Verified: `tsc` clean, biome clean, `bun test` 38/38. (Recompile blocked while
  `regent` is running — close it, then `bun run compile`.)

## 2026-06-30 — fix(cli): no empty reply boxes; one frame per turn

- A reasoning/tool-using model (minimax-m3) emits brief mid-turn text before each
  tool call; when that's only a stray/partial `<think>` tag it strips to nothing,
  yet `TranscriptItem` guarded on the *raw* text — so it drew an **empty `✦ Regent`
  box**, and substantive preambles got their own boxes too (multiple frames/turn).
- Fix: decide emptiness from the *rendered* content (`splitThinking`) — no visible
  answer or thinking → render nothing. Assistant entries now carry a `final` flag
  (set on `message.complete`); only the final reply is framed, mid-turn preambles
  render plain (one box per turn, like Hermes). New reducer test.
- Files: `regent-cli` (`transcript.ts` + test, `TranscriptItem.tsx`). Verified:
  `tsc` clean, biome clean, `bun test` 38/38, binary recompiled.

## 2026-06-30 — fix(cli): long replies no longer double (live-region overflow)

- A long reply showed **twice**: the first copy cut off mid-box (no bottom
  border), the second complete. Cause: the live streaming region is redrawn in
  place by Ink; once it grew taller than the viewport, Ink couldn't erase it and
  spilled the partial render into scrollback, then the committed `<Static>` entry
  printed the full copy. Wrapping the live region in the full box made it taller
  and worse.
- Fix: the live streaming region now shows a **bounded tail** (last few lines,
  clamped to the viewport) under a plain `✦ Regent` header — no full box — so it
  can never overflow. The complete framed box still renders once the reply commits
  to `<Static>`. (Pairs with the earlier reducer-level dedup for revised answers.)
- Files: `regent-cli/features/chat/presentation/ChatView.tsx`. Verified: `tsc`
  clean, biome clean, `bun test` 37/37, CLI binary recompiled.

## 2026-06-30 — feat(cli): full Hermes-style reply box + status bar (model · context · time)

- **Full rounded box.** `AssistantFrame` now draws all four sides with rounded
  corners and the label set into the top border (`╭─ ✦ Regent ──╮ │…│ ╰──╯`), like
  the Hermes reply box, instead of open top/bottom lines (teal border, gold label).
- **Status bar.** The status line is now a Hermes-style meta bar: `✦ <model> ·
  <used>/<max> [████░░░░] NN% · NNs` with a live elapsed timer while busy, plus the
  spinner/approval/idle state. Token counts render compactly (16.1K/524.3K).
- **Usage plumbing.** New `Agent::context_usage()` (estimated context tokens +
  budget, via the same estimator compression uses); `SessionManager::run_turn`
  emits a `turn.usage` notification after each turn; the CLI reducer stores it
  (`contextTokens`/`maxContextTokens`/`model`) and the status bar renders it.
- Files: `regent-agent` (`application/agent/mod.rs`), `regent-daemon`
  (`session_manager/mod.rs`), `regent-cli` (`transcript.ts` + test, `StatusLine.tsx`,
  `AssistantFrame.tsx`, `ChatView.tsx`). Verified: `cargo build -p regent-daemon`
  green; `tsc` clean; biome clean; `bun test` 37/37; CLI binary recompiled.

## 2026-06-30 — fix(cli): doubled reply + show response frame while streaming

- **Doubled response.** When the model revised its answer across tool rounds
  (e.g. streamed a 4-reference answer, committed it via a tool call, then
  re-streamed a 5-reference version), the `message.complete` dedup only dropped
  the earlier copy if the final reply was an *exact* `startsWith` prefix — a
  reworded word mid-text broke that, so both copies showed. Dedup now also
  collapses on a long *shared* prefix (`supersedes`), keeping the user-boundary
  guard so a prior turn is never touched. New regression test.
- **Frame visibility.** The Hermes-style top/bottom lines were only on the
  committed entry; the live streaming region showed bare text. Wrapped the live
  region in `AssistantFrame` too, so the `── ✦ Regent ──` frame appears as the
  reply streams. (If you didn't see the frame at all: the CLI binary needed a
  recompile — `bun run compile`.)
- Files: `regent-cli` (`features/chat/domain/transcript.ts` + test,
  `features/chat/presentation/ChatView.tsx`). Verified: `tsc` clean, biome clean
  on changed files, `bun test` 36/36, `bun run compile` rebuilt `dist/regent-cli.exe`.

## 2026-06-30 — feat(tools): reveal downloaded/created files in the file manager (Bug #5)

- Whenever the agent **makes a new file**, it now pops the OS file manager with
  the file selected — Explorer (`/select`) on Windows, Finder (`open -R`) on
  macOS, the FreeDesktop "show items" (else `xdg-open` the folder) on Linux. New
  `infra/reveal.rs`; `write_file` calls it only for **new** files (not in-place
  edits), best-effort and fire-and-forget (never fails the write). Burst-throttled
  (≤1 window / 2s) so a multi-file generation doesn't spam windows; off with
  `REGENT_REVEAL_FILES=0`. `play` opens a stream URL (no file), so it's untouched.
- Files: `regent-tools` (`infra/reveal.rs` new, `infra/mod.rs`, `infra/files.rs`).
  Verified: `cargo test -p regent-tools` (reveal env-parse + files 4/4). Needs a
  daemon rebuild to take effect.

## 2026-06-30 — feat(cli): Hermes-style top/bottom lines on agent responses (Bug #4)

- Recreated how the Hermes CLI brackets replies — Rich `box.HORIZONTALS`: a
  left-aligned labelled top line + a plain bottom line, no side borders. New
  `AssistantFrame` wraps the committed assistant reply with `── ✦ Regent ───…`
  (gold label, teal rules, content indented) above and a teal rule below. Sized
  once at render (committed history doesn't reflow, per the TUI resize model).
  Skips the frame for empty replies.
- Files: `regent-cli` (`features/chat/presentation/components/AssistantFrame.tsx`
  new, `TranscriptItem.tsx`). Verified: `tsc --noEmit` clean; biome clean on the
  new/changed files; `bun test` 35/35.

## 2026-06-30 — fix(agent): complete command knowledge + don't-overdo restraint (Bug #2)

- **Drift.** `CAPABILITIES` (the prompt's command list) was missing the whole
  **voice** group — `voice` (local ASR/TTS setup/enable/status/models/test) and
  `call` (live voice call) — both real router commands, so Regent didn't know they
  exist. Added them.
- **Supported vs not.** With the new `regent` tool, "supported" now means
  tool-runnable (daemon-backed); the prompt lists the hand-off-only ones
  (gateway, setup, doctor, config set, keys via manage_keys, auth, security,
  debug, mcp, logs).
- **Restraint.** `BASE_PROMPT` now says to do exactly what's asked and no more —
  no scope expansion, no unrequested steps/files, no extra tools "to be thorough";
  simplest path that fully answers, deeper only when asked.
- Files: `regent-agent/lib.rs`. Verified: `cargo build -p regent-agent` green.
  (Per-subcommand exhaustive audit deferred — the `regent` tool's param errors
  self-correct details; the structural group drift is fixed.)

## 2026-06-30 — feat(agent): `regent` tool — run your own admin commands in-process

- **Bug #1 (capability).** Regent had tools for keys/persona/memory/kanban/skills
  but nothing for model/status/config/cron/voice/agents/insights — so to do those
  it shelled out to `regent …` and deadlocked (see the terminal short-circuit).
- **Fix.** New core **`regent` tool** (`method` + `params`) forwards straight to
  the daemon's own JSON-RPC dispatcher **in-process** — the SAME handlers the CLI
  drives, so no second daemon, no store deadlock, no command-mapping duplication.
  `SessionManager::run_admin_command` builds a throwaway dispatcher over a local
  channel (skips notification lines, 120s guard); turn/session-lifecycle methods
  (`prompt.submit`, `turn.interrupt`, …) are refused so the agent can't drive its
  own live turn. Installed via `install_admin` at the composition root (cron +
  config + speech wired in); absent in tests → tool simply isn't registered. The
  agent now self-runs everything daemon-backed (per the user's "Everything"
  scope); gateway/setup/doctor/keys/config-set/auth/etc. have no daemon method, so
  it hands those to the user. Prompt (`BASE_PROMPT`/`CAPABILITIES`) updated to point
  at the tool.
- Files: `regent-daemon` (`application/regent_tool.rs` new, `session_manager/{mod,build}.rs`,
  `bin/regent-daemon.rs`, `application/mod.rs`, `lib.rs`), `regent-agent/lib.rs`.
  Verified: `cargo build -p regent-daemon` green; `cargo test -p regent-daemon`
  (22/22, incl. new `run_admin_command_routes_and_refuses_lifecycle`); clippy clean
  on new code. Rebuild + restart the daemon to expose the tool.

## 2026-06-30 — fix(tools): terminal no longer deadlocks on the `regent` CLI (the "snag")

- **Bug #1 (symptom).** Asking Regent to run one of its own `regent <command>`s
  made the terminal "hit a snag": the agent IS the running daemon, and shelling
  out to `regent` boots a **second** daemon that deadlocks on the shared SQLite
  store → 60s timeout → the generic tool-failure message. The system prompt asked
  it not to, but nothing enforced it.
- **Fix.** The `terminal` tool now detects a `regent`/`regent.exe` invocation
  (first word of the command or of any `&&`/`||`/`|`/`;`/newline segment) and
  short-circuits with guidance — use your own tools, or hand the user the exact
  `regent <command>` — instead of spawning the deadlocking daemon. Same root cause
  as the voice-call "stuck request"; this is the enforcement half.
- Files: `regent-tools/infra/terminal.rs`. Verified: `cargo test -p regent-tools
  terminal` (5/5, incl. detector + short-circuit). Follow-up (gated): a `regent`
  tool that runs the safe admin commands **in-process** so Regent can actually do
  them (model/status/skills/agents/…), not just hand off.

## 2026-06-30 — fix(call): stream the agent reply over stdio — TTS on sentence 1, no pileup

- **>2s before Regent spoke + stuck requests.** The browser call routed agent
  turns through the daemon's **buffered** HTTP `/v1/chat`, so `web_call.py` got
  the whole reply as one string (`iter([agent])`) — the per-sentence TTS loop had
  nothing to stream until the entire agentic turn finished. And a barge-in/stop
  aborted only the *browser* fetch; the daemon kept generating the abandoned turn,
  so the next turn queued behind it (the "stuck" requests).
- **Fix (one file, no daemon rebuild):** talk to the daemon over its own
  newline-delimited **JSON-RPC 2.0 stdio** transport (the same one the CLI uses).
  `prompt.submit` streams `message.delta` token-by-token, so TTS starts on
  sentence 1 while the model is still writing. Each new utterance first sends
  `turn.interrupt` (latest-wins), so an abandoned turn is cancelled instead of
  blocking the next one. Falls back to the plain streaming completion
  (`_brain_stream`) when the daemon/model isn't available. Removed the dead HTTP
  client (`_agent_reply`/`_ensure_agent`) + the now-unused `secrets`/`_clean_reply`.
- Files: `python-voice-server/web_call.py`. Verified: `python web_call.py`
  self-check (JSON-RPC line router) green; `ast.parse` clean; no stale refs;
  `warm_agent`/`register_call_routes` surface unchanged. Restart the speech server
  to pick it up. Note: the daemon binary is reused as-is (no Rust change).

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
- **browser control via Playwright MCP (opt-in, approval-gated).** The same mechanism leading
  coding agents use: point `REGENT_BROWSER_MCP_URL` at a running Playwright(-compatible) MCP server and the
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
- **Reference policy:** the reference agent's Ink source is studied for craft/patterns only and reimplemented
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
  architecture mapping, and a capability comparison against a leading agent's sandbox runtime and the
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
(a local sibling checkout), replace the Node orchestration plane with Tokio
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

**Goal:** (A) Study the Hermes Agent repository (`NousResearch/hermes-agent`, from a local
copy) and document how it works and interconnects; (B) propose the full
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
