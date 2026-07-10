# File-length audit — src/crates files over 200 lines

Generated 2026-07-10 (`wc -l` on every `.rs` under `src/crates`, threshold >200).
House rule: target ≤ ~200 lines per file — split or justify in one sentence.

## Remediated 2026-07-10 (top 8, all tests green before/after)

The eight worst offenders were split along feature seams, public APIs kept
identical via module re-exports:

| Was | File | Now |
|---:|---|---|
| 998 | regent-deacon/tests/deacon_basics.rs | `deacon_basics/` — main + helpers, rpc_types, sessions, dispatcher_basic, dispatcher_admin, dispatcher_models, turns, routing (32 tests, unchanged) |
| 965 | regent-deacon …/dispatcher/admin_ops.rs | per-feature `*_ops.rs`: skills, memory, model, mom, cron, cron_edit, status, persona, kanban, agents |
| 562 | regent-tools/src/infra/key_tool.rs | `key_tool/` — mod (tool), catalog (data), env_file (.env primitives). catalog.rs is 224 lines: one flat const table of managed keys, splitting the list would hurt readability |
| 478 | regent-agent/tests/agent_loop.rs | `agent_loop/` — main + helpers, turn_flow, interrupts, resume (9 tests, unchanged) |
| 475 | regent-voice-server/src/infra/http.rs | `http/` — mod (state/router), security, pages, audio, call, tests |
| 428 | regent-deacon …/dispatcher/voice_ops.rs | voice_ops (status/models/test), voice_set_ops, voice_weights_ops, speech_yaml |
| 425 | regent-speech/src/infra/remote.rs | `remote/` — mod (request building), asr, tts, tests |
| 402 | regent-gateway …/platforms/wechat.rs | `wechat/` — mod (adapter), media (file sends), tests |

## Remaining (from the 2026-07-10 scan)

| Lines | File |
|---:|---|
| 388 | src/crates/regent-tools/src/application/catalog.rs |
| 385 | src/crates/regent-gateway/src/infra/platforms/wecom.rs |
| 363 | src/crates/regent-deacon/src/application/dispatcher/session_ops.rs |
| 351 | src/crates/regent-deacon/src/infra/webhook/tests.rs |
| 349 | src/crates/regent-deacon/src/infra/webhook.rs |
| 341 | src/crates/regent-gateway/src/infra/platforms/feishu.rs |
| 339 | src/crates/regent-kernel/src/contracts/speech.rs |
| 339 | src/crates/regent-graph/src/application/orchestrators.rs |
| 336 | src/crates/regent-gateway/src/infra/platforms/whatsapp.rs |
| 330 | src/crates/regent-tools/src/infra/memory_tools.rs |
| 329 | src/crates/regent-agent/src/domain/prompts.rs |
| 327 | src/crates/regent-gateway/src/bin/gateway.rs |
| 324 | src/crates/regent-speech/src/models.rs |
| 323 | src/crates/regent-deacon/src/infra/discord_interactions.rs |
| 320 | src/crates/regent-voice-server/src/application/turn.rs |
| 317 | src/crates/regent-deacon/src/bin/regent-deacon.rs |
| 316 | src/crates/regent-deacon/src/application/dispatcher/env_ops.rs |
| 313 | src/crates/regent-voice-server/src/infra/deacon.rs |
| 312 | src/crates/regent-deacon/src/application/session_manager/queries.rs |
| 311 | src/crates/regent-tools/src/infra/play.rs |
| 308 | src/crates/regent-deacon/src/application/session_manager/build.rs |
| 307 | src/crates/regent-tools/src/infra/kanban_tools.rs |
| 303 | src/crates/regent-gateway/src/infra/platforms/slack.rs |
| 293 | src/crates/regent-deacon/src/application/provider_registry.rs |
| 292 | src/crates/regent-gateway/src/infra/platforms/discord.rs |
| 290 | src/crates/regent-agent/src/application/board/runner.rs |
| 289 | src/crates/regent-gateway/src/infra/platforms/google_chat.rs |
| 286 | src/crates/regent-deacon/src/application/speech_factory.rs |
| 282 | src/crates/regent-deacon/src/application/dispatcher/artifacts_ops.rs |
| 278 | src/crates/regent-tools/src/infra/message_tools.rs |
| 281 | src/crates/regent-deacon/src/application/dispatcher/mod.rs |
| 274 | src/crates/regent-deacon/src/domain/config/provider_catalog.rs |
| 266 | src/crates/regent-deacon/src/infra/webhook/registry.rs |
| 266 | src/crates/regent-code/src/domain/mod.rs |
| 264 | src/crates/regent-orchustr-core/or-mcp/src/multi_client.rs |
| 262 | src/crates/regent-gateway/tests/gateway_behavior.rs |
| 261 | src/crates/regent-tools/src/infra/backends.rs |
| 259 | src/crates/regent-tools/src/infra/vision_analyze.rs |
| 259 | src/crates/regent-gateway/src/infra/platforms/email.rs |
| 253 | src/crates/regent-voice-server/src/domain/vad.rs |
| 252 | src/crates/regent-tools/src/infra/file_ops.rs |
| 252 | src/crates/regent-agent/src/application/mom/mod.rs |
| 247 | src/crates/regent-tools/src/infra/web_search.rs |
| 242 | src/crates/regent-tools/src/infra/computer_use/mod.rs |
| 241 | src/crates/regent-kernel/src/types/transcript.rs |
| 237 | src/crates/regent-tools/src/infra/terminal.rs |
| 237 | src/crates/regent-store/src/infra/sessions.rs |
| 236 | src/crates/regent-tools/src/infra/mcp_tools.rs |
| 235 | src/crates/regent-graph/tests/golden_retrieval.rs |
| 233 | src/crates/regent-store/src/infra/graph.rs |
| 231 | src/crates/regent-speech/src/registry.rs |
| 230 | src/crates/regent-tools/src/infra/sandbox.rs |
| 227 | src/crates/regent-store/src/infra/schema.rs |
| 226 | src/crates/regent-store/src/infra/db.rs |
| 226 | src/crates/regent-gateway/src/infra/platforms/trello.rs |
| 226 | src/crates/regent-gateway/src/infra/platforms/azure_devops.rs |
| 226 | src/crates/regent-deacon/src/application/session_manager/mod.rs |
| 225 | src/crates/regent-gateway/src/infra/platforms/messenger.rs |
| 225 | src/crates/regent-gateway/src/infra/platforms/jira.rs |
| 225 | src/crates/regent-deacon/src/domain/config/provider_kind.rs |
| 224 | src/crates/regent-tools/src/infra/control_app.rs |
| 224 | src/crates/regent-tools/src/infra/key_tool/catalog.rs |
| 224 | src/crates/regent-skills/tests/library_behavior.rs |
| 223 | src/crates/regent-code/src/application/harness.rs |
| 222 | src/crates/regent-gateway/src/domain/contracts.rs |
| 219 | src/crates/regent-providers/src/infra/openai_stream.rs |
| 219 | src/crates/regent-gateway/src/application/runner.rs |
| 218 | src/crates/regent-cron/tests/scheduler_behavior.rs |
| 217 | src/crates/regent-store/src/infra/embeddings.rs |
| 217 | src/crates/regent-realtime/src/lib.rs |
| 216 | src/crates/regent-tools/src/infra/video_analyze.rs |
| 216 | src/crates/regent-tools/src/infra/mcp_server.rs |
| 216 | src/crates/regent-agent/tests/delegation.rs |
| 216 | src/crates/regent-agent/src/bin/repl.rs |
| 216 | src/crates/regent-agent/src/application/delegation/tool.rs |
| 215 | src/crates/regent-skills/src/application/library.rs |
| 211 | src/crates/regent-store/src/infra/kanban.rs |
| 210 | src/crates/regent-gateway/src/infra/platforms/twilio_sms.rs |
| 210 | src/crates/regent-gateway/src/infra/platforms/telegram/voice.rs |
| 209 | src/crates/regent-agent/src/application/agent/mod.rs |
| 208 | src/crates/regent-tools/src/infra/checkpoint.rs |
| 207 | src/crates/regent-tools/src/infra/files.rs |
| 207 | src/crates/regent-graph/src/application/evals.rs |
| 206 | src/crates/regent-code/tests/harness_flow.rs |
| 205 | src/crates/regent-tools/src/infra/skill_tools.rs |
| 205 | src/crates/regent-providers/src/infra/openai_compat.rs |
| 204 | src/crates/regent-store/tests/store_roundtrip.rs |
| 203 | src/crates/regent-skills/src/infra/fs_repository.rs |
| 203 | src/crates/regent-gateway/src/infra/platforms/twilio_voice.rs |
