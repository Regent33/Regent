# Regent Desktop — Handoff to New Conversation (2026-07-08)

**Branch:** `feat/constitutional-prompt` · **HEAD:** `dd79c4b` · **App:** `src/regent-app/Desktop`
(Next 16 static export + Tauri 2, deacon stdio-RPC per ADR-033).

> Parallel sessions are also committing here — reconcile with `git log` before large edits.
> `dd79c4b` (Main Models settings + fallback chain + Code button center), `af4a393`
> (Butler mic picker, hide internal sessions), and BOM fixes already partially touch some
> items below — **verify current state before rebuilding from scratch.**

## How to run / verify
- Rust: workspace manifest under `src/`. `cargo test -p regent-deacon -p regent-tools -p regent-skills -p regent-store`.
  Release binary the app spawns: `cargo build --release -p regent-deacon` — **fails with `os error 5` while the
  desktop app is open (it locks the exe); close the app first.**
- Desktop: from `src/regent-app/Desktop` → `bun run typecheck`, `bun run build`, `bun test`.
- Drive the deacon directly (diagnosis): spawn `target/(debug|release)/regent-deacon.exe` with
  `REGENT_HOME=<home>` + `REGENT_NOW=<iso>` + the `.env` vars merged (the app spawner merges `$REGENT_HOME/.env`;
  a bare run does NOT). Send JSON-RPC lines on stdin. Pattern scripts in the scratchpad.

## Key architecture facts (verified this session)
- Config schema: `src/crates/regent-deacon/src/domain/config/` (split into mod/model/runtime/speech + `provider_kind.rs`).
  `providers` is a **name-keyed map** `{<name>: {kind, base_url?, api_key_env, models:[]}}`, NOT a list.
  `agents_defaults: {primary: ModelRef?, fallbacks: [ModelRef]}`. `mom: {<name>: {proposers:[], aggregator, max_proposers}}`.
- 17 `ProviderKind`s, each with correct `key_env_var()` + `openai_base_path() -> (base, api_path)` (Gemini/Zhipu/Perplexity
  use non-standard paths). Main-provider key resolves via `ProviderKind::resolve_key()` (own env var, then `REGENT_API_KEY`).
- `config.set {path,value}` validates the whole file against `DeaconConfig` before writing (can't brick startup).
- `env.* {list,set,unset}` manage `.env` keys (masked, values never returned). `manage_keys` tool = agent-facing equivalent.
- Constitution is **forced on at load** (`config_loader` sets `enabled=true`); never disableable.
- Main interactive chat currently builds a **single** provider (`make_provider_factory`); the `FallbackChat` chain is
  only wired to the board/agent path via `agents_defaults.fallbacks` (see item #3).

## Done this session (committed)
M7 (event bus, overlays, settings kit, skills restyle), M8 (chat shiki/embeds/attachments, rail actions, status bar v2),
M9 (dark theme, embeds, notifications, keybinds). Backend: `config.set`, `env.*`, 17 providers, `artifacts.list/get`,
provider-aware key resolution, `session.rename/pin/archive/delete`, title-gen, turn-usage on the wire. Bugs fixed:
ollama-cloud config brick, 404 (base_url/model), archived-skill view, tab/close collision, **UTF-8 BOM in `.env`
hiding the first key**. Config repaired + chat verified E2E against Ollama Cloud (in isolation).

---

# TASKS (P0 first)

## TASK — P0: Chat is not working in the app
### CONTEXT
User reports chat non-functional in the desktop app. In isolation a `prompt.submit` against the user's config returned
a clean reply (Ollama Cloud, `minimax-m3`), so the deacon path works — suspect the **running app spawned a stale release
binary** (rebuild was repeatedly blocked by the exe lock) and/or a frontend regression in `useChatSession`/`Composer`
after the M8 rewrite. Also the running deacon was spawned before the `.env` BOM fix, so its env was corrupted.
### Reference
`features/chat/viewmodels/useChatSession.ts`, `features/chat/presentation/{ChatView,Composer}.tsx`,
`shared/state/deaconBus.ts`, `src-tauri/src/deacon/{spawn.rs,rpc.rs}`. Deacon: `dispatcher/session_ops.rs::prompt_submit`.
### Expected Output
Close app → `cargo build --release -p regent-deacon` (clean) → reopen → send a message → streamed reply renders.
If still broken, capture the exact on-screen error + the `deacon-event` stream and fix root cause (do not mask).

## TASK — P0: All past sessions unavailable; message roles/turns not matched
### CONTEXT
Past sessions don't list, and when they do the transcript doesn't pair user/assistant turns correctly. `session.list`
returns rows; `session.history` returns stored messages (rows carry `role`, `text`, `reasoning`, `tool_calls`).
The transcript reducer must map each stored message to its role so conversation turns render in order.
### Reference
`features/shell/viewmodels/useSessions.ts`, `features/chat/viewmodels/useChatSession.ts::rowToItems`,
`shared/kernel/transcript.ts`. Deacon: `session_ops.rs::session_list/session_history`, `regent-store` conversations.
### Expected Output
The rail lists all past sessions; opening one seeds the full transcript with correctly-ordered user/assistant turns
(and thinking/tool rows). Verify against a real store with many sessions (user has ~800).

## TASK — P0: Whole page scrolls (vertical AND horizontal) instead of the inner panel
### CONTEXT
The app window/page itself moves on scroll; wide content (code blocks, tables) causes horizontal body scroll. The shell
must be a fixed 100vh/100vw frame with `overflow: hidden`; only inner panels scroll (`overflow-y-auto`), and wide
content scrolls inside its own `overflow-x-auto` container.
### Reference
`app/globals.css` (html/body height), `features/shell/presentation/Shell.tsx`, `shared/ui/Overlay.tsx`,
`shared/ui/Markdown.tsx` (code blocks need `overflow-x:auto`). Overlays already fixed-inset; check the base shell + chat pane.
### Expected Output
Body never scrolls; only the intended inner panel scrolls; no horizontal page scroll on any surface.

## TASK — 1: Move Main-models sections (Primary, Secondary, Fallbacks) to the Model page
### CONTEXT
`agents_defaults.primary` + `agents_defaults.fallbacks` (each a `ModelRef {provider, model}`) are the primary +
ordered fallback chain. Surface them on the Model settings page (Primary, Secondary=first fallback, Fallbacks list),
editable via `config.set` (paths under `agents_defaults`). Partly started in `dd79c4b` — verify/finish.
### Reference
`features/settings/presentation/ModelSection.tsx` + `MainModelPicker.tsx`; config paths `agents_defaults.primary`,
`agents_defaults.fallbacks`. `config.get.providers` supplies selectable provider/model pairs.
### Expected Output
Model page shows Main model + Primary/Secondary/Fallbacks; edits persist via config.set (validated); reflected in config.get.

## TASK — 2: API Keys page must include gateway/platform + other provider types, not just LLM
### CONTEXT
`env.list` currently returns only LLM provider keys (`env_ops.rs::LLM_KEYS`). The API Keys page should group ALL managed
key types: **LLM providers**, **Gateway/Messaging platforms** (Telegram/Slack/Discord/WhatsApp/Messenger/LINE/
Mattermost/Twilio/Teams/Feishu/WeChat/WeCom/Mailgun/Jira/Azure DevOps/Trello/GChat), **Search** (Brave/Tavily/SerpAPI/
Exa/Google CSE), **Speech/Vision**. The full set is `key_tool.rs::MANAGED`.
### Reference
`src/crates/regent-deacon/src/application/dispatcher/env_ops.rs` (extend `env.list` to return grouped keys, or add a
`group` field per key), `regent-tools/src/infra/key_tool.rs::MANAGED`. UI: `features/settings/presentation/ApiKeysSection.tsx`.
### Expected Output
API Keys page shows collapsible groups (LLM / Messaging / Search / Speech), each row set/replace/remove via env.set/unset.

## TASK — 3: Provider fallback + dynamic re-routing on failure + MOM must work
### CONTEXT
When the primary provider rate-limits (429) or errors (5xx/network/auth), the chat must auto-roll to the next
`agents_defaults.fallbacks` entry (`FallbackChat` fails over on those, NOT on 4xx client errors). Today the **interactive
chat path builds a single provider** (`make_provider_factory`); the fallback chain is only wired to the board path.
Wire `provider_registry::chain_for(primary, fallbacks)` into the interactive turn. Also verify MOM (`mom.run`) works end-to-end.
Started in `dd79c4b` ("recovering provider fallback chain") — verify it actually re-routes on 429.
### Reference
`regent-deacon/src/application/{provider_factory.rs,provider_registry.rs,session_manager/build.rs}`,
`regent-providers` `FallbackChat` + `fallback_chain.rs` tests. `dispatcher/admin_ops.rs::mom_run`.
### Expected Output
Force a 429 on the primary → the turn transparently completes on a fallback; if ALL fail, the final error surfaces
verbatim. `mom.run` returns aggregated output. Add a test that a 429 fails over but a 404 does not.

## TASK — 4: Code page input — smaller, centered, dynamic-grow with inner scroll; smaller popups
### CONTEXT
The Code page task input should be a smaller, centered box that grows with content up to a cap, then scrolls INSIDE
itself. Also make the overlay popup pages smaller. Code button centering partly done in `dd79c4b`.
### Reference
`features/code/presentation/CodeView.tsx`, `features/chat/presentation/Composer.tsx` (auto-grow pattern with
MAX_ROWS + inner scroll), `shared/ui/Overlay.tsx` (panel max-width/height).
### Expected Output
Centered auto-growing input with inner scroll past the cap; overlays visibly smaller.

## TASK — 5: Session CRUD (create / rename / delete) works end-to-end
### CONTEXT
`session.create`, `session.rename`, `session.delete`, `session.pin`, `session.archive` exist (RPC batch A). Verify the
rail actions actually call them and update the list optimistically + on refetch.
### Reference
`features/shell/presentation/SessionRow.tsx` + `viewmodels/useSessions.ts`; deacon `dispatcher/session_admin_ops.rs`.
### Expected Output
Create/rename/delete a session from the rail; state persists across reload (verified against the store).

## TASK — 6: Input-token efficiency
### CONTEXT
First-turn input ~20k. Deferred-tools (`tools.deferred`, `catalog.defer`) already withholds ~12 heavy tool schemas.
Baseline is high due to ~25 non-deferred tool schemas + the skills index + constitution + retrieved memory. Reduce:
audit which tools/skills are always sent; consider deferring more by default; measure the real breakdown.
### Reference
`regent-deacon/src/application/session_manager/build.rs` (`catalog.defer`), `regent-agent` prompt assembly,
`config.tools.{disabled,deferred}`. Constitution now always-on ADDS tokens.
### Expected Output
A measured token breakdown + a reduction (e.g. more default-deferred tools, or trimmed skills index) with chat unaffected.

## TASK — 7: Agent-driven API-key/provider edits must use the compatible schema/format
### CONTEXT
When the user asks the agent to set keys/providers, it must (a) write keys via the `manage_keys` tool / `env.set`
(never freehand `.env`), and (b) configure providers with the correct `kind`/`base_url`/`api_key_env`/`models` and the
right per-provider base+path (see `provider_kind.rs::openai_base_path`). The `regent` tool description already steers
`config.set`; extend guidance so provider entries use the exact schema + the 17 known kinds' conventions.
### Reference
`regent-deacon/src/application/regent_tool.rs` (tool description), `provider_kind.rs`, `config_ops.rs`.
### Expected Output
Asking the agent "use Groq with llama-3.3-70b" produces a valid `providers.groq` entry + `GROQ_API_KEY` guidance,
validated by config.set, that actually works.

## TASK — 8: Slash `/` menu + all slash commands available in chat, app sessions, AND Code
### CONTEXT
`commands.list` + the composer slash menu shipped in M8 for chat. Ensure the same `/` menu + command execution works
in every session surface and in the Code page input.
### Reference
`features/chat/presentation/Composer.tsx` + `viewmodels/useSlashCommands.ts`, `features/code/presentation/CodeView.tsx`,
deacon `commands.list`. Hermes reference: `app/session/hooks/use-prompt-actions/slash.ts`.
### Expected Output
Typing `/` in chat and in Code shows the command menu; commands execute (or route) correctly in both.

## TASK — 9: (folded into P0 "Chat is not working")

## TASK — 10: (folded into P0 "Whole page scrolls")

## TASK — 11: (folded into P0 "Past sessions + message roles")

## TASK — 12: API Keys — allow multiple keys of the same provider
### CONTEXT
Hermes shows "N keys" per provider (rotation / multiple accounts). Today `env.set` is one value per env-var name.
Support multiple keys per provider — e.g. `OPENROUTER_API_KEY`, `OPENROUTER_API_KEY_2`, … or a list — with the runtime
picking/rotating. Requires a backend convention + UI to add/list/remove N keys per provider.
### Reference
`env_ops.rs`, `key_tool.rs`, provider key resolution (`ProviderKind::resolve_key`). UI: `ApiKeysSection.tsx`/`ApiKeyRow.tsx`.
### Expected Output
Add ≥2 keys for one provider; both stored (masked); runtime uses/rotates them.

## TASK — 13: Remove the rectangle focus outline on every input when selected
### CONTEXT
The global focus ring (`globals.css` `:focus-visible { outline: 2px solid var(--accent) }`) draws a rectangle on inputs.
Inputs should show a subtler affordance (border-color change / underline), not the 2px accent rectangle. Keep a11y
(don't remove focus indication entirely — replace with border/ring).
### Reference
`app/globals.css` (the `:where(a,button,input,textarea,select,[tabindex]):focus-visible` rule), `shared/ui/SearchField.tsx`
(already uses underline), settings `fields.tsx` `CONTROL` (uses `focus:border-accent`).
### Expected Output
No rectangular outline on text inputs on focus; a clean border/underline affordance instead; a11y preserved.

## TASK — 14: Bottom/status-bar icons should open popup pages (Hermes parity)
### CONTEXT
Hermes status-bar items open popover panels (gateway state, agents, cron, model menu, context/usage). Regent's status
bar has some (model menu shipped M8); make every bottom icon open its panel.
### Reference
`features/shell/presentation/StatusBar.tsx` + `StatusBarModelMenu.tsx`; Hermes `app/shell/{gateway-menu-panel,
context-usage-panel,model-menu-panel}.tsx`.
### Expected Output
Clicking each status-bar item opens a token-styled popover with the relevant info/actions.

---

# Remaining planned work (from the M7–M10 plan + this session)
- **Skills & Tools full-width grouped redesign** (screenshot fidelity: category chips w/ counts, grouped sections,
  per-row toggle). Agent hit session limit — do inline. Files: `features/skills/**`, `en.ts` skills block. Current
  `SkillsView` is master-detail; target is the Hermes full-width grouped list.
- **Release rebuild + app restart** so all backend fixes (constitution always-on, BOM/spawn, providers, env.*) reach
  the running app.
- **M10 remainder:** Artifacts viewer UI (backend `artifacts.list/get` done) + right sidebar panes; memory-graph
  overlay (needs regent-graph edges/timestamps — deferred).
- **M6 packaging:** NSIS installer, real brand assets, KONTES license decision, updater.

**Carried rules:** additive-only deacon changes; tokens-only styling; files ≤ ~200 lines; Result-typed calls; errors
verbatim (never mask); per-milestone `bun test`/`tsc`/`next build`/`cargo test`; conventional commits on the user's branch.
