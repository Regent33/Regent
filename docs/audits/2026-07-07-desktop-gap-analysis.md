# Regent Desktop vs Hermes Desktop — Gap Analysis & Implementation Plan

**Date:** 2026-07-07 · **Basis:** full file-tree + source study of
`hermes-agent-main/apps/desktop/src` (~430 TS/TSX files) vs `src/regent-app/Desktop`
(~70 files, M0–M5 shipped through `46721d4`). Hermes is Electron+Vite+React on a
gateway WebSocket; Regent is Next+Tauri on the deacon stdio-RPC (ADR-033). This
compares **capability and UX structure**, not stacks.

---

## Part 1 — Gap analysis

### 1. Architecture / state
| Hermes | Regent today | Gap |
|---|---|---|
| ~50 Zustand store modules (`store/session`, `composer`, `activity`, `notifications`, `layout`, `panes`, `updates`, `windows`…) — app-wide shared state, event-driven | Per-feature hooks, state dies with the component | **P1.** No cross-surface state: status bar can't show turn activity, chat state resets on route change, nothing observes the deacon stream globally |
| Gateway WebSocket + `gateway-events` — one event bus feeding all stores | One `deacon-event` Tauri channel, subscribed per-hook | **P1.** Need a single shared event-bus layer (one `onDeaconEvent` fan-out store) instead of N subscriptions |
| Per-session state cache (`use-session-state-cache`) + virtualized thread/list | Full remount per session; plain map over rows | **P2.** Long transcripts will jank; session switch loses scroll |
| Route **overlays** framework (`app/overlays/*`, `use-overlay-routing`) — Settings/Skills/palette/model-picker float OVER chat | Everything is a route; navigating loses chat context | **P1.** Settings/Skills/pickers should be overlays (Hermes-fidelity + keeps the session alive behind) |

### 2. Chat surface
| Hermes | Regent today | Gap |
|---|---|---|
| assistant-ui stack: markdown + **shiki code highlighting**, diff-lines, ansi-text, expandable blocks, zoomable images, generated-image results | react-markdown + GFM, plain `<pre>` | **P1.** Code blocks unhighlighted; no image rendering in chat; no expand/collapse for long tool output |
| **Embeds subsystem** with consent gate: YouTube/Spotify/Maps/Twitter/TikTok/Instagram/Pinterest/Vimeo + Mermaid + SVG + iframe | Links are plain anchors (Butler has result cards) | **P2.** Inline rich embeds in chat (start: YouTube + maps + mermaid) |
| Composer: rich editor, `@`-mentions + `/`-command completions, **attachments** (drop overlay, previews), mic recorder + voice conversation, **model pill hot-swap**, message **queue panel**, status stack (coding/preview rows), input history | Textarea + send/stop; attach & mic are inert placeholders | **P0.** Attachments (needs a deacon upload/attach RPC — additive), model hot-swap pill (model.set exists), slash/@ completions (commands.list exists), input history. Mic-in-chat after Butler mic is solid |
| Tool approval **groups**, tool-fallback rendering with args/result summaries (`tool-result-summary`), clarify-tool | Tool rows are name+spinner only; approval card exists | **P1.** Tool rows should disclose args/result summaries (tool.start/complete would need payload — additive deacon change) |
| Scroll-to-bottom button, activity timer, skeletons, intro | Auto-scroll only | **P2** |
| Session title generation + rename/actions menu (pin, archive, delete, export) | Rows show `source · id`; no actions | **P0.** Needs additive deacon RPCs: `session.rename/pin/archive/delete` + title-gen (Hermes has Title-gen aux model); rail already live |

### 3. Shell
| Hermes | Regent today | Gap |
|---|---|---|
| Status bar built from composable `use-statusbar-items`: gateway state menu, agents, cron, **token/context % meter**, session timer, **model menu panel** (picker + edit submenu), version → updates overlay | Static items; model text-only; ctx/agents/cron are "—" | **P0.** Wire context % (needs token usage on the wire — `insights.get` per-session or turn usage events), model picker menu (model.list/set exist), cron/agents counts (cron.list, agents.list exist) |
| Right sidebar: **files tree** (project), **review pane** (diff churn/ship), **terminal**, **preview pane** | Right-panel button is inert | **P2/P3.** Biggest structural gap; needs deacon fs/terminal RPCs (security-gated). Start with Preview (artifacts) |
| Sidebar: pinned + drag reorder, virtualized list, session actions menu, projects/workspaces, cron section, profile switcher | Flat live list | **P1.** Pin/reorder + actions menu after the session RPCs land |
| Keybinds system (combo parser, panel, per-action map) | Hardcoded ⌘K/Esc/Ctrl+N-hint | **P2** |
| Onboarding overlay, boot-failure overlay (+reauth), gateway-connecting overlay, notifications (+native), completion sound, updates overlay + changelog | Boot splash only | **P1:** boot-failure state (deacon dead → actionable overlay instead of silent "—"). **P2:** native notifications on turn-complete-in-background, completion sound. **P3:** updates overlay (ties to M6 updater) |
| Full **theming** (presets, user themes, VSCode import, per-profile) + language switcher (4 locales) | Light tokens only (dark structured-for) | **P1:** ship dark ramp (GATE said light-first *then dark*). **P3:** presets/user themes/locales |

### 4. Settings (user-flagged)
Hermes: per-section modules + shared `primitives.tsx` field kit; **search-settings**
with deep-link highlight; providers CRUD + provider-config panels; env credentials
vault; computer-use panel; toolset config; MCP; notifications; appearance; sessions
maintenance; uninstall. Regent: 4 real sections (Model/Voice/Memory/About) + roadmap
stubs; no search; no primitives kit; Apply not centered; **user reports runtime
failures** (need exact error text; suspect result-shape drift — verify each RPC
against dispatcher source).
**Gap: P0.** Order: fix broken actions → settings `primitives.tsx` equivalent (Field/
Row/Section components) → search + deep-link → main-model + auxiliary-models layout
per reference → computer-use toggle panel (`REGENT_COMPUTER_USE`) → providers/keys
(config.get + .env writes — needs additive deacon `config.set`/`env.set`).

### 5. Skills & Tools (user-flagged, reference provided)
Hermes: top search, **Skills/Toolsets tabs**, category chips with counts, grouped
rows with **enable/disable switches**, toolset-config panel. Regent: master-detail +
tools list; view works (usage-ledger crash fixed in `regent-skills`).
**Gap: P0.** Search + tabs + Switch primitive + toggle via `skills.opt_out` (verify
an opt-in inverse exists; add additive `skills.opt_in` if not). Categories need skill
frontmatter tags — additive `skills.list` field.

### 6. Butler (Regent-only flagship — no Hermes equivalent)
Ahead of Hermes here (voice stage, map backdrop, auto result cards). Remaining: mic
device picker (current bug shows silent default device; enumerate + persist choice),
window snap-docking, camera frames port (`/call/frame`), presenter events from the
agent itself (deacon `present.*` notifications — additive) instead of regex intent.

### 7. Missing entirely (deliberate, unbuilt)
Artifacts viewer (deacon has `$REGENT_HOME/artifacts` — needs an additive `artifacts.list/get` RPC) ·
Messaging surface (gateway platforms exist in deacon) · projects/workspaces ·
session export · todos surface · terminal · review pane · tray/floating HUD · pet (skip).

---

## Part 2 — Implementation plan (M7+, verified per milestone as before)

**M7 — Foundation for parity (unblocks everything)**
1. Shared deacon event-bus store (one subscription fanning out; session-scoped selectors).
2. Overlay framework (`shared/ui/Overlay*`: chrome, search input, split layout; route-param driven) — move Settings/Skills/palette into overlays.
3. Settings primitives kit + **fix every broken settings action** against dispatcher source; search + centered Apply per reference.
4. Skills page restyle: search/tabs/chips/toggles (+ `skills.opt_in` additive RPC if needed).
5. Additive deacon RPCs batch A: `session.rename/pin/archive/delete`, `skills.opt_in`, tool event payloads (args/result summary on `tool.start/complete`).

**M8 — Chat parity core**
1. Shiki highlighting + expandable blocks + zoomable images in Markdown.
2. Composer v2: model hot-swap pill, slash/@ completions (commands.list), input history, scroll-to-bottom, activity timer.
3. Attachments end-to-end (additive deacon `attachment.put` + prompt refs).
4. Session titles (title-gen on first turn — aux model call in deacon, additive) + rail actions menu (pin/rename/archive/delete) + pinned section w/ drag reorder.
5. Status bar v2: model menu panel, context % meter (turn usage events), cron/agents counts, boot-failure overlay.

**M9 — Dark theme + embeds + notifications**
1. Dark token ramp (GATE follow-through), Appearance section Light/Dark/System.
2. Embeds with consent gate: YouTube, Maps, Mermaid first.
3. Native notifications + completion sound on background turn completion.
4. Keybinds map + panel.

**M10 — Right panel & artifacts**
1. Artifacts RPCs (additive) + Artifacts page + right-panel preview pane.
2. Right sidebar shell (panes store) with Preview; terminal/review deferred behind it.
3. Butler: mic device picker, snap docking, camera frames, `present.*` agent events.

**M6 (unchanged, can run parallel):** NSIS installer, real brand assets, KONTES
license decision, updater (keys + host) + updates overlay.

**Rules carried forward:** additive-only deacon changes, tokens-only styling,
Result-typed calls, errors verbatim, per-milestone `bun test`/`tsc`/`next build`/
`cargo test`/`tauri build` + audits. Delegation: Sonnet for spec'd UI batches, Opus
for the event-bus/overlay framework — subject to subagent limits (all hit today;
inventory partial output and finish inline when it happens).

---

## Addendum (2026-07-07 pm) — updated Hermes reference

The reference app gained: **starmap memory graph** (`/journey`, aliases
`/learning`,`/memory-graph` — `app/starmap`: canvas force-sim of skills +
memories over a time axis, node context menu, share codes), the **IDE right
sidebar** now concrete (`app/right-sidebar/{files,review,terminal}` + panes
store), and shell panels (model-menu, context-usage, keybinds, command-center).

Plan deltas:
- M8 already covers model menu + context meter + slash completions (in flight).
- M9 unchanged (keybind-panel now has direct reference code).
- **M10 grows**: right-sidebar work uses the new reference; ADD "memory graph
  overlay" fed by memory.list / regent-graph (Regent-real data; start with the
  graph + time axis, skip share codes). Pet surfaces stay skipped.
