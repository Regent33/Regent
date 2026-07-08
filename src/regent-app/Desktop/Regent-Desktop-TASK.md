<!-- Drop into the `## TASK` slot of MASTER PROMPT v3. The operating loop, gates,
     architecture, and security all come from the master prompt — not restated here.
     REVISED 2026-07-06 after code-grounded architecture review (see §Backend seam). -->

# TASK — Regent Desktop App

Build the **Regent Desktop app**: a native desktop client for the Regent agent, on
**Next.js + Tauri**, in `src/regent-app/Desktop/` of this repo. Greenfield — nothing to
preserve there yet; follow the repo's canonical `app / shared / features` architecture
(§2), adapted per §Architecture-adaptation below.

## Reference — read before building

- `D:\1-1@k\1-1 Hermes Agent\hermes-agent-main\hermes-agent-main\apps\desktop\DESIGN.md` + `\README.md`
- `D:\1-1@k\@ServeAI\Regent APP\INSPO\` (product screens + `Butler Mode\`)
- Hermes desktop is **Electron + Vite + React** — a *different stack*. Treat it as a
  **design / UX / information-architecture template, not a code template**: port its
  DESIGN.md discipline (tokens-over-literals · one-primitive-per-concern · flat-not-boxed
  · borderless + shadow for elevation · ~100ms functional motion · reduced-motion
  respected) and its layout. **Do not** copy Electron/Vite code onto Tauri.

**IA to adapt from Hermes** (rename to Regent): left rail (New session · Skills & Tools ·
Messaging · Artifacts · Search · Pinned · Sessions) · top bar (session-title dropdown
left; audio/account/settings/right-panel + window controls right) · composer (attach ·
"Send follow-up" · mic · animated voice orb; stop button while streaming) · bottom status
bar (gateway · agents · cron · token/context % · session timer · model picker · version) ·
full-screen Settings (Model · Chat · Appearance · Workspace · Safety · Memory & Context ·
Voice · Advanced · Gateway · API Keys · MCP · Archived · About) · Profiles (list + SOUL.md
editor) · faint full-bleed watermark behind the main pane.

## Backend seam (code-grounded — was wrong in v1 of this plan)

Verified against the repo 2026-07-06:

- **`regent-deacon` is THE agent backend.** Newline-delimited **JSON-RPC 2.0 over
  stdio**, spawned as a child process per client — exactly how `regent-cli`
  (`shared/infrastructure/deacon/spawn.ts`) and `regent-voice-server`
  (`infra/deacon.rs`, a Rust port of the same client) already connect. The dispatcher
  already exposes everything the desktop needs: `sessions.*`, `status.get`,
  `insights.get`, `persona.get`, `memory.*`, `model.get`, `config.get`, voice ops,
  admin/skills ops, and **`code.plan` / `code.start`** for the Code page. Streamed
  events (`RpcEvent::Delta/Reply/End`) **carry `session_id` — the client MUST filter
  by it** (regression fixed 2026-07-06; do not reintroduce).
- **`regent-web` is NOT a backend.** It is a thin Next.js *client* of the voice server
  (the call page). Nothing routes through it.
- **Butler voice = `regent-voice-server` HTTP API** (`/call/token`, `/call/turn`,
  `/call/frame` on localhost) — the same API `regent-web/hooks/localCall.ts` consumes.
  The webview calls it directly (CORS already handled server-side). The desktop app
  **resolves the prebuilt binary** (like `voiceServe.ts`: prefer `target/release`, env
  override) — it never builds the voice server (needs LLVM/libclang).

**Desktop transport (proposed, confirm at GATE):** the Tauri Rust core spawns the
deacon (stdio JSON-RPC, port of `DeaconRpc`), exposes typed `invoke` commands for
request/response and Tauri **events** for streamed deltas (filtered by `session_id`).
The webview never spawns processes and never gets shell/fs capabilities. Butler voice
goes webview → voice-server HTTP directly (CSP `connect-src` allows that localhost
port only).

**Chat wire contract (verified in dispatcher/mod.rs + domain/rpc.rs, 2026-07-06):**
requests — `session.create` / `session.resume` / `session.list` / `session.search`
(note: SINGULAR `session.`), `prompt.submit` {session_id, text}, `turn.interrupt`,
`approval.respond` {session_id, approved}; streamed notifications — `message.delta`
{session_id, text} → `message.complete` {session_id, reply} (non-streaming providers)
→ `turn.complete` | `turn.interrupted` {session_id, error?}; `turn.started` exists but
is ignorable. Model/status: `model.get/list/set`, `status.get`, `insights.get`. The
full method catalogue lives at dispatcher/mod.rs:83-135 (kanban, agents, skills,
tools, memory, providers, voice, cron, code, mom, persona, config, commands).

**Process lifecycle:** deacon spawned at app launch with `REGENT_HOME` + `.env` merge
(mirror `spawn.ts` semantics incl. `REGENT_NOW`), killed on exit with the same
2s-grace drain; respawn-on-death like the voice server does. Voice server spawned
hidden on first Butler entry, reused if already on its port — **beware the stale-binary
trap** (CHANGELOG 2026-07-06): after voice-server changes, rebuild release AND kill the
running process. Single-instance plugin on the Tauri app.

## Stack (fixed) — one lane each, no overlap

| Layer | Choice | Lane |
|---|---|---|
| Framework | Next.js (App Router) + TypeScript | UI only; **static export** (`output: 'export'`), SPA |
| Desktop shell | Tauri v2 | window, IPC, packaging, auto-update |
| Styling | Tailwind CSS | tokens → utilities; no raw hex in components |
| Timeline motion | GSAP | choreographed sequences, particle-core states, page transitions |
| Interaction motion | React Spring | gesture/physics springs (drag, floating windows) |
| Scroll | Lenis | smooth scroll only — **install only if CSS `scroll-behavior` proves insufficient** |
| 3D / particles | Three.js | Butler particle core, grid field |

**Install-at-first-use:** M0/M1 carry only Next+Tauri+Tailwind. Three.js + GSAP land at
M3a, React Spring at M3b. No dependency ships before the milestone that uses it.

**Critical constraint:** Tauri serves a **static** bundle — no Next SSR, server actions,
route handlers, or ISR at runtime. All agent calls go over the seam above — never a Next
server. Target platform for v1 is **Windows** (NSIS + WebView2); mac/linux later.

## Architecture adaptation (this app is a thin RPC client — trim §2 accordingly)

- `shared/kernel` — `Result`, branded ids, RPC message types (mirror deacon contracts).
- `shared/infrastructure/rpc` — ONE typed wrapper over Tauri `invoke` + event
  subscription. All IPC flows through it; inputs validated here (both sides).
- `shared/ui` — tokens + primitives (Button, SearchField, SegmentedControl, ListRow,
  Loader, ErrorState, EmptyState, LogView — the Hermes primitive set, Regent-skinned).
- `features/<x>/presentation` + `viewmodels` (hooks/stores). A `domain/` layer only
  where real logic exists (transcript assembly, code-run state machine) — **do not**
  pre-generate empty domain/data/DI trees per feature. No DI container; module imports.
  Rust side: `src-tauri` stays a thin bridge (spawn, RPC pump, command handlers) —
  no agent logic in the shell.
- Provider errors (402 no-credit, 401, 429) surface verbatim in chat + call UI —
  never swallowed (house rule).
- i18n: one `en.ts` strings module + `t()` helper. No library until a second locale.

## Design language

- **Tokens** (CSS vars, referenced everywhere — never literals): `--bg` `#E4DDD3` (warm
  bone) · `--accent` `#00A19B` (teal). Derive the neutral text/stroke/hover/elevation ramp
  from these. Appearance offers Light/Dark/System like Hermes — dark-mode token values
  proposed at GATE, not hardcoded blindly.
- **Font:** KONTES Compressed Bold — self-host via `next/font/local`. **File not yet
  provided** → wire a documented condensed-grotesque fallback behind a single swap point.
  Big display wordmark on the empty-home hero (teal on bone), like Hermes' "HERMES AGENT".
- **Logo & app icon:** not yet designed → placeholder `BrandMark` (one swappable asset) +
  `TODO`; do not block on it.
- **Feel:** minimal, flat, generous whitespace, hairline dividers, borderless + shadow
  elevation; fluid-but-functional motion; **`prefers-reduced-motion` honored beyond a
  plain fade**.

## Surfaces (full app — each a milestone, gated per §1.3)

1. **Shell** — Tauri window + custom titlebar, left rail, status bar, command palette
   (⌘K), theme/mode provider, watermark. Status-bar items bind to real RPC
   (`status.get`, `insights.get`, `model.get`) — no fake data.
2. **Home** — empty-state hero (wordmark + one-line pitch) → composer.
3. **Chat / Session** — streaming transcript with Thinking + tool-call rows
   (Product/Technical toggle), voice-orb composer. Session list/search via `sessions.*`.
4. **Butler / Presenter Mode** *(flagship — split below)*.
5. **Code** — Claude-Code-like page bound to **`code.plan` / `code.start`** deacon RPC
   (plan → verify → revert): plan view · diff/approve gates · run log · revert.
6. **Skills & Tools** · **Artifacts** · **Messaging** · **Cron / Routines** · **Profiles**
   (SOUL.md editor) · **Settings** (the Hermes section set above) · **About**.

## Butler / Presenter Mode (signature screen — three sub-milestones)

- **M3a — Call core:** full-screen toggled "Jarvis" view = **Regent Call**, on the
  voice-server HTTP API. Kinetic particle core (Three.js), centered — glowing
  ring/particle system reacting to speech amplitude & state (idle → listening →
  speaking), per `INSPO\Butler Mode\`. Faded animated grid background fading at edges.
  Keepalive-aware: long silent thinks stream `keepalive` lines — don't reset UI state.
- **M3b — Floating windows:** React Spring physics, draggable, presenting apps /
  projects / searches / objects / images / people — docking cluster like the inspo.
- **M3c — Map + chart explainer:** built-in fluid map (places/history) + visual chart
  explainer. **Open dependency:** map tiles need a network tile source (CSP allowance)
  or offline tiles — resolve when M3c starts, not at GATE. Charts 2D-first.

## Acceptance (per milestone)

- `next build` static export clean · `tauri build` yields a launchable app · `tauri dev`
  runs.
- Zero raw colors / one-off shadows in components (tokens only) · reduced-motion path
  exists · a11y baseline (focus rings, input labels, ≥44px targets, AA contrast) · Esc
  closes overlays.
- IPC inputs validated at the boundary · webview gets **no** shell/fs capability ·
  Tauri v2 capabilities least-privilege · CSP locked to the voice-server port · no
  secrets in the bundle · user-facing strings i18n-routed · streamed events filtered
  by `session_id`.

## Milestone order (self-gate each; >3 files ⇒ plan first)

M0 scaffold (Next+Tauri+Tailwind, static export proven, deacon spawn+`status.get`
round-trip over IPC) → M1 shell + design-system primitives + tokens → M2 Home + Chat +
composer → M3a Butler call core → M3b floating windows → M3c map/charts → M4 Code page →
M5 Settings/Profiles/rest → M6 packaging + icon/font swap-in (auto-update **deferred**
unless signing keys + update host exist — placeholder wiring only). Execution is per
pair: `M0 → M1`, `M1 → M2`, etc.

## Delegation (decided — models per task)

- **Fable (orchestrator, this session):** architecture, gates, plan reviews, cross-agent
  integration, final verification per milestone.
- **Opus 4.8 agents** (complex/novel): Tauri Rust core + `DeaconRpc` port + IPC bridge
  (M0/M1) · design-token system + primitive set (M1) · Butler particle core +
  audio-reactive state machine (M3a) · floating-window physics/docking (M3b) · Code
  page run state machine (M4).
- **Sonnet 5 agents** (well-specified, from an approved spec/primitives): M0 Next+Tailwind
  scaffold config · M2 transcript rows/composer components · session list/search wiring ·
  M5 Settings/Profiles forms · i18n plumbing · M6 packaging config + swap points.
- Every agent works from a written brief (files + contracts + acceptance); output is
  reviewed against DESIGN.md discipline + this plan before merge. Agents never touch
  crates outside `src/regent-app/Desktop/`.

## GATE — confirmed 2026-07-06

1. **Backend/IPC seam** — ✅ approved as §Backend-seam above (deacon stdio JSON-RPC via
   Tauri Rust core; Butler voice = `regent-voice-server` HTTP from the webview;
   `regent-web` is not a seam).
2. **KONTES font** — provided at `C:\FONTS\Kontes Font\kontes-compressed-bold.ttf`.
   ⚠️ License readme says **personal use only** — fine for a personal build; commercial
   distribution needs the paid license. Wired via one `@font-face` swap point with a
   condensed-grotesque fallback stack; the binary font file stays **gitignored** so the
   repo never redistributes it (a fresh clone silently falls back).
3. **Dark mode** — **light first, then dark**: v1 ships light-only, but tokens stay
   dual-value-structured so dark lands later without touching components.
4. **Logo/app icon** — temporary placeholder now (`BrandMark` + generated icon set);
   real assets supplied later, drop-in at M6.

## Plan review — 2026-07-08 (post-gap-list session)

Reviewed the 14-task gap list against the code before executing; three of its premises
were wrong, so the plan is corrected here rather than executed as written:

- **"Chat is not working"** was environmental, not code: an orphaned stale release
  deacon held the exe lock and predated the BOM/env fixes, and the frontend static
  export had never been rebuilt. Kill + rebuild + reopen fixed it — no chat rewrite.
- **"Message roles/turns not matched"** was NOT a transcript-reducer bug. Two real
  causes: `session.list` defaulted to 20 rows with the internal-source filter applied
  after the limit (588 curator "review" sessions flooded out real chats), and
  `Agent::resume` bricked any session whose history contained a crashed turn
  ("two user messages in a row"). Both fixed (limit 1000; resume now repairs).
- **"No typing animation"** (found during the session): every OpenAI-compatible
  provider lacked a streaming impl — the trait default emits one delta at turn end.
  Real SSE streaming shipped in `openai_stream.rs`.

Shipped this session: the three P0s, focus-ring softening (task 13), dark-mode Shiki,
resume-repair, SSE streaming, composer overlay (chat flows behind the input pill),
rail restructure (fixed nav head, scrolling session list, SESSIONS collapse showing 7
when collapsed), Main models moved to the Model page (task 1).

Deferred: task 12 (multiple keys per provider) — no runtime consumer yet; revisit when
key rotation is a real story.

Delegation for the remaining tasks (session limit permitting):
- **Opus 4.8**: task 2 (grouped `env.list` + API-keys UI groups), task 3 verify (429
  fails over / 404 does not; MOM e2e), task 6 (token breakdown + more default-deferred
  tools).
- **Sonnet 5**: task 4+8 (Code input + slash menu, shared files), task 14 (status-bar
  popovers), task 5 UI verify, Skills & Tools full-width grouped redesign (Hermes
  parity: category chips w/ counts, grouped sections, per-row toggles).
