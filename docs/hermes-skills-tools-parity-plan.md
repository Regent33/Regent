# Regent — Hermes Skills & Tools Parity Plan

**Status: PLAN (2026-06-20).** Companion to [cli-command-parity-plan.md](./cli-command-parity-plan.md)
(which covers *commands*). This plan covers the **tool catalog** and the **skill library** — bringing
Regent toward [Hermes Agent](../../1-1%20Hermes%20Agent) parity by building **real tools** (no stubs),
and deciding how Regent's *skills* relate to Hermes' pre-built skill packs.

## Why a plan and not a wholesale port

Hermes ships **~32 tool families** and **~37 skill categories** (18 bundled + 19 optional). Recreating
all of them is multi-week work spanning new crates (browser/computer-use, media generation, several
third-party integrations) and a product decision about Regent's skill model. Per the user's
instruction ("recreate ALL … if not create A FULL detailed plan in docs"), this is that plan: an
inventory, a gap analysis, a phased build order, and the open decisions.

## Principles (binding — same as the command plan)

1. **No stubs.** A tool ships only when it actually performs its function and is unit-tested.
2. **Logic in the owning crate**, exposed through `regent-tools` (the `ToolCatalog` + `DispatchHook`
   seam), registered per-session in the daemon's `build.rs`. Same three layers as today.
3. **Provider-backed where Hermes is** (image/tts/etc. call a provider) — route through
   `regent-providers`, keep keys in `.env`, never inline.
4. **Per-tool gate:** crate capability (+ tests) → catalog registration → tool definition (name,
   schema, description) visible to the model → `cargo test --workspace` + `clippy -D warnings` green.
5. **Edges before core**, reuse before build: several Hermes tools already exist in Regent.

## Tool inventory → Regent status

Legend — ✅ exists · 🟡 partial · 🔵 build (extend existing crate) · 🟠 build (new crate/subsystem).

| Hermes tool(s) | Capability | Regent status | Owning crate / notes |
|---|---|---|---|
| `file_tools`, `terminal_tool`, `read_terminal_tool` | read/write/search files, run + read terminal | ✅ | `regent-tools` core (`read_file`, `search_files`, `write_file`, `terminal`) |
| `memory_tool` | long-term memory read/write | ✅ | `regent-graph` + memory tools |
| `kanban_tools` | board CRUD + worker dispatch | ✅ | `regent-store` kanban + kanban tool (see CLI `kanban`) |
| `skills_tool`, `skill_manager_tool` | view/create/install skills | ✅ | `regent-skills` + skill tools |
| `cronjob_tools` | scheduled jobs | ✅ | `regent-cron` (see CLI `cron`) |
| `delegate_tool` | spawn a sub-agent for a subtask | ✅ | `regent-agent` delegation (`delegate_task`) |
| `send_message_tool` | proactive outbound message | ✅ | gateway delivery (`send_message`) |
| `mcp_tool` | call tools on an MCP server | ✅ | `or-mcp` / `regent-tools` MCP bridge |
| `session_search_tool` | search past sessions | 🟡 | `session.search` exists as a command; expose as a model **tool** |
| `clarify_tool` | ask the user a question mid-turn | 🔵 | `regent-tools` — reuse the approval/event channel to round-trip a prompt |
| `todo_tool` | per-turn todo list the model maintains | 🔵 | `regent-tools` + a tiny store table (or in-session state) |
| `code_execution_tool` | run code in a sandbox | 🟠 | new `regent-sandbox` (container/subprocess isolation) — security-sensitive |
| `web_tools`, `x_search_tool` | web search + fetch, X/Twitter search | 🟠 | new `regent-web` (search API + fetch/extract); provider keys in `.env` |
| `browser_tool`, `browser_cdp_tool`, `browser_dialog_tool` | headless browser automation (CDP) | 🟠 | new `regent-browser` (CDP client) — large |
| `computer_use_tool` | screen/mouse/keyboard control | 🟠 | new `regent-computer-use` — large, OS-specific, security-sensitive |
| `image_generation_tool`, `video_generation_tool` | media generation | 🟠 | `regent-providers` media routes (fal/openai/etc.) |
| `tts_tool`, `transcription_tools` | text-to-speech, speech-to-text | 🟠 | `regent-providers` audio routes |
| `vision_tools` | image understanding | 🔵 | `regent-providers` — many chat models are already multimodal; wire image inputs |
| `mixture_of_agents_tool` | ensemble several models, synthesize | 🔵 | `regent-agent` — fan-out to providers + reduce |
| `discord_tool` | Discord read/post | 🟠 | `regent-gateway` already has a Discord adapter — promote to a model tool |
| `homeassistant_tool` | smart-home control | 🟠 | new integration (HA REST API) |
| `feishu_doc_tool`, `feishu_drive_tool`, `yuanbao_tools` | Feishu docs/drive, Yuanbao | 🟠 | per-integration; likely **out of scope** unless requested |

**Tally:** ~10 of 32 already in Regent; ~3 quick (🟡/🔵: session-search-as-tool, clarify, todo, vision,
MoA); the rest are real subsystems (sandbox, web, browser, computer-use, media) or niche integrations.

## Skill library — the model difference (decision required)

Hermes ships **pre-authored skill packs** (markdown skill files) in categories: *apple,
autonomous-ai-agents, creative, data-science, devops, email, github, media, mlops, note-taking,
productivity, research, smart-home, social-media, software-development, …* (+ optional: blockchain,
communication, finance, gaming, health, migration, security, web-development).

Regent's current model (ADR / next-steps): **skills are learned from reviewed sessions** and stored
in `regent-skills`, surfaced via `skills view/create/opt-out`. So "recreate ALL Hermes skills" is a
**product fork**, not just porting:

- **Option A — ship skill packs (Hermes-style).** Port a curated subset of Hermes' SKILL.md files into
  `regent-skills` as seeded, provenance-tagged ("bundled") skills, installable via `bundles install
  <name>` (see command plan B5.1). Pre-loads capability; keeps the learn-from-sessions path too.
- **Option B — keep learn-from-sessions only.** Don't pre-ship packs; the curator grows skills from
  use. Less out-of-box capability, simpler, on-brand with the current architecture.
- **Recommendation:** **A, curated.** Seed a handful of high-value packs (software-development,
  research, devops, github) as bundles; defer the long tail. This needs the `bundles` command (B5.1)
  as the delivery vehicle, and an ADR for the bundle format + trust/provenance.

## Phased build order

- **T0 — expose what exists (days):** `session_search` as a model tool; `clarify_tool` (reuse the
  approval round-trip); `todo_tool`; wire `vision` image inputs. Low risk, high model utility.
- **T1 — web (P-edges):** `regent-web` — search + fetch/extract (`web_search`, `web_fetch`), then
  `x_search`. Keys in `.env`. Unlocks research skills.
- **T2 — media (provider routes):** image/video generation, tts, transcription via `regent-providers`.
- **T3 — mixture_of_agents:** ensemble + synthesize in `regent-agent`.
- **T4 — sandboxed code execution:** `regent-sandbox` (gets its own ADR — isolation model, resource
  limits, network policy). Security-sensitive; gated behind explicit approval.
- **T5 — browser / computer-use:** `regent-browser` (CDP) then `regent-computer-use`. Largest; each
  its own ADR. Approval-gated.
- **T6 — integrations:** promote the gateway Discord adapter to a tool; Home Assistant; evaluate
  Feishu/Yuanbao (likely out of scope).
- **S1 — skills (parallel, after `bundles`):** seed curated skill packs as bundles (Option A).

## Verification (every tool)

`cargo test --workspace` + `cargo clippy --all-targets -- -D warnings`; a unit test exercising the
tool's `dispatch`; the tool appears in `regent tools` and in a session's catalog. ADR + CHANGELOG when
a tool constrains future work (sandbox, browser, computer-use, skill-bundle format).

## Risks / open questions

- **Skill model fork** (A vs B above) — needs a user/product decision before S1.
- **Security surface:** code execution, browser, and computer-use are powerful; all must be
  approval-gated through the existing `DispatchHook`/approval channel, off by default.
- **Provider sprawl:** media/tts/web pull in several third-party APIs; keep each behind a config flag
  and `.env` key, and degrade gracefully when unconfigured (the tool simply isn't registered).
- **Scope:** Feishu/Yuanbao and some optional skill categories may stay out of scope unless requested.
