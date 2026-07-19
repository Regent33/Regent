# File-size audit — 2026-07-19

Policy: source files should stay **≤200 lines**; **≤250 is still acceptable**.
Files **>250** are split candidates. Vendored third-party code and legacy
(superseded) code are **excepted** — not counted against the rule.

Scan: `git ls-files '*.rs' '*.ts' '*.tsx' '*.py'` (so `target/`, `node_modules/`,
and other build output are already excluded), files >200 lines only.

**Totals (>200 lines):** 130 files —
33 source >250 · 66 source 201–250 ·
23 test · 5 vendored (excepted) ·
3 legacy (excepted).

Known intentional exceptions inside the lists below (splitting harms cohesion):
`Installer/src-tauri/src/wire.rs` (3-platform cfg-gated, only Windows
compile-verifiable), `prompts/system.rs` (prompt-string constants — one
artifact), `i18n/en/settings.ts` (translation strings), `config/model_lists.rs`
& `domain/model_windows.rs` (model catalog data), `create_document/theme.rs`
(theme presets), `config/provider_kind.rs` (provider enum + endpoints).

---

## Over 250 — source split candidates (33)

| Lines | File |
|---:|---|
| 564 | `src/regent-app/Desktop/features/butler/data/callLoop.ts` |
| 485 | `src/regent-app/Desktop/features/butler/viewmodels/useButlerCall.ts` |
| 436 | `src/regent-app/Installer/src-tauri/src/wire.rs` |
| 435 | `src/crates/regent-agent/src/application/agent/turn.rs` |
| 435 | `src/regent-app/Desktop/features/chat/viewmodels/useChatSession.ts` |
| 434 | `src/regent-app/Desktop/features/chat/viewmodels/useSpeechToText.ts` |
| 316 | `src/regent-cli/src/features/documents/runtime/presentation.ts` |
| 312 | `src/regent-cli/src/features/voice/cli/voiceServe.ts` |
| 308 | `src/crates/regent-providers/src/infra/adapters.rs` |
| 304 | `src/crates/regent-tools/src/infra/create_document/images.rs` |
| 303 | `src/crates/regent-deacon/src/application/session_manager/hooks.rs` |
| 302 | `src/regent-app/Desktop/features/settings/presentation/VoiceSection.tsx` |
| 301 | `src/regent-app/Desktop/features/butler/presentation/DiagramBackdrop.tsx` |
| 299 | `src/crates/regent-voice-server/src/application/turn.rs` |
| 295 | `src/crates/regent-providers/src/application/orchestrators.rs` |
| 292 | `src/regent-app/Desktop/features/chat/data/localCommands.ts` |
| 291 | `src/regent-app/Desktop/features/graph/presentation/GraphCanvas.tsx` |
| 290 | `src/crates/regent-graph/src/application/retrieve.rs` |
| 288 | `src/crates/regent-tools/src/infra/key_tool/env_file.rs` |
| 286 | `src/crates/regent-tools/src/infra/computer_use/mod.rs` |
| 282 | `src/regent-app/Desktop/features/chat/presentation/Composer.tsx` |
| 278 | `src/regent-app/Desktop/features/settings/presentation/MainModelsSection.tsx` |
| 275 | `src/crates/regent-deacon/src/domain/config/runtime.rs` |
| 275 | `src/crates/regent-voice-server/src/application/turn/synth.rs` |
| 274 | `src/crates/regent-deacon/src/application/session_manager/build.rs` |
| 271 | `src/crates/regent-voice-server/src/domain/vad.rs` |
| 267 | `src/crates/regent-deacon/src/application/dispatcher/session_ops.rs` |
| 266 | `src/crates/regent-voice-server/src/infra/deacon.rs` |
| 263 | `src/crates/regent-tools/src/infra/create_document/theme.rs` |
| 259 | `src/regent-app/Installer/src-tauri/src/lib.rs` |
| 256 | `src/regent-app/Desktop/features/butler/presentation/ButlerView.tsx` |
| 254 | `src/regent-app/Installer/app/App.tsx` |
| 251 | `src/crates/regent-store/src/infra/sessions.rs` |

---

## 201–250 — acceptable, monitor (66)

| Lines | File |
|---:|---|
| 249 | `src/crates/regent-gateway/src/infra/platforms/discord.rs` |
| 249 | `src/crates/regent-gateway/src/infra/platforms/whatsapp.rs` |
| 248 | `src/crates/regent-deacon/src/domain/config/provider_kind.rs` |
| 247 | `src/regent-app/Desktop/features/settings/viewmodels/useMainModels.ts` |
| 246 | `src/crates/regent-tools/src/infra/create_document/renderer.rs` |
| 245 | `src/regent-app/Desktop/shared/diagram/presentValidate.ts` |
| 244 | `src/crates/regent-gateway/src/infra/platforms/wecom.rs` |
| 242 | `src/crates/regent-store/src/infra/schema.rs` |
| 241 | `src/crates/regent-deacon/src/application/session_manager/session_ctx.rs` |
| 241 | `src/crates/regent-tools/src/infra/key_tool/catalog.rs` |
| 238 | `src/crates/regent-deacon/src/application/dispatcher/prompt_ops.rs` |
| 237 | `src/regent-cli/src/features/persona/cli/personaCommand.ts` |
| 234 | `src/regent-cli/src/features/call/cli/callServe.ts` |
| 234 | `src/regent-cli/src/features/chat/presentation/ChatView.tsx` |
| 233 | `src/crates/regent-tools/src/infra/create_document/mod.rs` |
| 231 | `src/regent-cli/src/features/setup/presentation/SetupWizard.tsx` |
| 230 | `src/crates/regent-deacon/src/application/dispatcher/voice_set_ops.rs` |
| 230 | `src/crates/regent-tools/src/infra/persona_tool.rs` |
| 228 | `src/crates/regent-gateway/src/infra/platforms/slack.rs` |
| 228 | `src/crates/regent-tools/src/infra/create_document/pdf.rs` |
| 228 | `src/crates/regent-tools/src/infra/everyday/random_gen.rs` |
| 228 | `src/crates/regent-tools/src/infra/key_tool/mod.rs` |
| 227 | `src/crates/regent-code/src/application/harness.rs` |
| 227 | `src/crates/regent-voice-server/src/infra/spawn.rs` |
| 226 | `src/crates/regent-agent/src/domain/prompts/system.rs` |
| 225 | `src/regent-app/Desktop/shared/i18n/en/settings.ts` |
| 224 | `src/crates/regent-store/src/domain/entities.rs` |
| 223 | `src/crates/regent-deacon/src/infra/discord_interactions.rs` |
| 222 | `src/crates/regent-gateway/src/domain/contracts.rs` |
| 220 | `src/crates/regent-agent/src/domain/compression.rs` |
| 220 | `src/crates/regent-providers/src/infra/openai_stream.rs` |
| 219 | `src/crates/regent-gateway/src/application/runner.rs` |
| 218 | `src/crates/regent-agent/src/bin/repl.rs` |
| 218 | `src/crates/regent-deacon/src/domain/ledger.rs` |
| 218 | `src/crates/regent-providers/src/infra/anthropic_chat.rs` |
| 217 | `src/crates/regent-tools/src/infra/play/resolve.rs` |
| 216 | `src/crates/regent-deacon/src/application/session_manager/code.rs` |
| 215 | `src/crates/regent-tools/src/infra/mcp_tools.rs` |
| 214 | `src/crates/regent-tools/src/infra/play/youtube.rs` |
| 214 | `src/regent-app/Desktop/features/settings/presentation/fields.tsx` |
| 213 | `src/crates/regent-deacon/src/application/constitution.rs` |
| 213 | `src/crates/regent-deacon/src/domain/config/model_lists.rs` |
| 213 | `src/crates/regent-providers/src/domain/model_windows.rs` |
| 212 | `src/crates/regent-voice-server/src/infra/sherpa.rs` |
| 211 | `src/crates/regent-skills/src/infra/fs_repository.rs` |
| 210 | `src/crates/regent-tools/src/domain/entities.rs` |
| 209 | `src/crates/regent-providers/src/infra/openai_compat.rs` |
| 209 | `src/crates/regent-tools/src/infra/everyday/convert.rs` |
| 208 | `src/crates/regent-skills/src/application/library.rs` |
| 207 | `src/crates/regent-store/src/infra/meta.rs` |
| 207 | `src/crates/regent-store/src/infra/persona.rs` |
| 207 | `src/regent-app/Desktop/shared/state/deaconBus.ts` |
| 206 | `src/regent-cli/src/features/cron/cli/cronCommand.ts` |
| 206 | `src/regent-cli/src/features/setup/cli/setupCommand.ts` |
| 205 | `src/crates/regent-speech/src/models.rs` |
| 205 | `src/crates/regent-tools/src/infra/skill_tools.rs` |
| 205 | `src/crates/regent-tools/src/infra/vision_analyze.rs` |
| 204 | `src/crates/regent-tools/src/infra/file_edit.rs` |
| 203 | `src/crates/regent-tools/src/infra/create_document/pptx_slide.rs` |
| 203 | `src/regent-cli/src/features/agents/cli/agentsCommand.ts` |
| 202 | `src/crates/regent-agent/src/application/agent/mod.rs` |
| 202 | `src/crates/regent-deacon/src/infra/config_loader.rs` |
| 201 | `src/crates/regent-deacon/src/application/dispatcher/artifacts_ops.rs` |
| 201 | `src/crates/regent-gateway/src/bin/gateway/conversations.rs` |
| 201 | `src/crates/regent-tools/src/infra/backends.rs` |
| 201 | `src/crates/regent-voice-server/src/infra/engines.rs` |

---

## Tests > 200 (23)

Tests get more leeway than source; listed for awareness, low split priority.

| Lines | File |
|---:|---|
| 571 | `src/crates/regent-tools/src/infra/create_document/create_document_tests.rs` |
| 464 | `src/crates/regent-deacon/tests/deacon_basics/tiering.rs` |
| 446 | `src/crates/regent-agent/tests/learning_loop.rs` |
| 414 | `src/crates/regent-agent/tests/agent_loop/turn_flow.rs` |
| 413 | `src/crates/regent-providers/tests/fallback_chain.rs` |
| 329 | `src/crates/regent-skills/tests/library_behavior.rs` |
| 295 | `src/crates/regent-deacon/tests/deacon_basics/sessions.rs` |
| 273 | `src/crates/regent-agent/src/domain/compression_tests.rs` |
| 265 | `src/crates/regent-gateway/tests/gateway_behavior.rs` |
| 261 | `src/crates/regent-store/tests/store_roundtrip.rs` |
| 249 | `src/crates/regent-deacon/tests/deacon_basics/turns.rs` |
| 245 | `src/crates/regent-agent/tests/agent_loop/resume.rs` |
| 238 | `src/crates/regent-deacon/tests/deacon_basics/ledger.rs` |
| 235 | `src/crates/regent-graph/tests/golden_retrieval.rs` |
| 231 | `src/crates/regent-deacon/tests/deacon_basics/dispatcher_models.rs` |
| 227 | `src/crates/regent-agent/tests/memory_integration.rs` |
| 219 | `src/crates/regent-tools/src/infra/everyday/reminder_tests.rs` |
| 218 | `src/crates/regent-cron/tests/scheduler_behavior.rs` |
| 216 | `src/crates/regent-agent/tests/delegation.rs` |
| 214 | `src/crates/regent-tools/src/application/catalog_tests.rs` |
| 213 | `src/crates/regent-deacon/src/infra/webhook/tests.rs` |
| 209 | `src/crates/regent-code/tests/harness_flow.rs` |
| 201 | `src/crates/regent-deacon/tests/token_budget.rs` |

---

## Excepted — vendored third-party (5)

`paddle-ocr-rs` (PaddleOCR Rust port) and `regent-orchustr-core` (vendored
or-core / or-mcp, ADR-005/008). Not subject to the rule.

| Lines | File |
|---:|---|
| 410 | `src/crates/paddle-ocr-rs/src/db_net.rs` |
| 324 | `src/crates/paddle-ocr-rs/src/ocr_lite.rs` |
| 264 | `src/crates/regent-orchustr-core/or-mcp/src/multi_client.rs` |
| 217 | `src/crates/paddle-ocr-rs/src/tests.rs` |
| 206 | `src/crates/paddle-ocr-rs/src/crnn_net.rs` |

---

## Excepted — legacy / superseded (3)

`python-voice-server` (superseded by the Rust regent-voice-server) and
`regent-web` (superseded by the Desktop app). Kept as fallback/reference; not
active code.

| Lines | File |
|---:|---|
| 687 | `python-voice-server/web_call.py` |
| 340 | `python-voice-server/python_server.py` |
| 295 | `src/regent-web/hooks/localCall.ts` |
