<!-- Drop into the `## TASK` slot of MASTER PROMPT v3. The operating loop, gates,
     architecture, and security all come from the master prompt — not restated here. -->

# TASK — Regent Desktop App

Build the **Regent Desktop app**: a native desktop client for the Regent agent, on
**Next.js + Tauri**, in the (currently empty) `src/regent-app/Desktop/` folder of this
repo. Greenfield — nothing to preserve there yet; follow the repo's canonical
`app / shared / features` architecture (§2).

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

## Stack (fixed) — one lane each, no overlap

| Layer | Choice | Lane |
|---|---|---|
| Framework | Next.js (App Router) + TypeScript | UI only; **static export** (`output: 'export'`), SPA |
| Desktop shell | Tauri v2 | window, IPC, packaging, auto-update |
| Styling | Tailwind CSS | tokens → utilities; no raw hex in components |
| Timeline motion | GSAP | choreographed sequences, particle-core states, page transitions |
| Interaction motion | React Spring | gesture/physics springs (drag, floating windows) |
| Scroll | Lenis | smooth scroll only |
| 3D / particles | Three.js | Butler particle core, grid field, map/chart 3D |

**Critical constraint:** Tauri serves a **static** bundle — no Next SSR, server actions,
route handlers, or ISR at runtime. All backend/agent calls go over **Tauri IPC / the
existing Regent core** (`src/regent-web`, `src/regent-cli`, `regent-voice-server`) — never
a Next server. Confirm the exact transport at GATE.

## Design language

- **Tokens** (CSS vars, referenced everywhere — never literals): `--bg` `#E4DDD3` (warm
  bone) · `--accent` `#00A19B` (teal). Derive the neutral text/stroke/hover/elevation ramp
  from these. Appearance offers Light/Dark/System like Hermes — propose dark-mode token
  values at GATE, don't hardcode a second palette blindly.
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
   (⌘K), theme/mode provider, watermark.
2. **Home** — empty-state hero (wordmark + one-line pitch) → composer.
3. **Chat / Session** — streaming transcript with Thinking + tool-call rows
   (Product/Technical toggle), voice-orb composer.
4. **Butler / Presenter Mode** *(flagship — below)*.
5. **Code** — Claude-Code-like page bound to **regent-code** (`regent code`: plan → verify
   → revert): plan view · diff/approve gates · run log · revert. Bound over IPC to the
   regent-code harness.
6. **Skills & Tools** · **Artifacts** · **Messaging** · **Cron / Routines** · **Profiles**
   (SOUL.md editor) · **Settings** (the Hermes section set above) · **About**.

## Butler / Presenter Mode (signature screen)

- Full-screen, toggled conversational "Jarvis" view = **Regent Call** (voice), tied to
  `regent-voice-server`.
- **Kinetic particle core** (Three.js), centered — glowing ring/particle system reacting
  to speech amplitude & state (idle → listening → speaking), per the JARVIS refs in
  `INSPO\Butler Mode\`.
- **Faded grid background** (like regent-web), gently animated, fading at edges.
- **Floating windows** (React Spring physics, draggable) presenting apps / projects /
  searches / objects / images / people — docking cluster like the inspo.
- **Built-in map** (fluid, smooth) for places & history queries.
- **Built-in visual chart explainer.**

## Acceptance (per milestone)

- `next build` static export clean · `tauri build` yields a launchable app · `tauri dev`
  runs.
- Zero raw colors / one-off shadows in components (tokens only) · reduced-motion path
  exists · a11y baseline (focus rings, input labels, ≥44px targets, AA contrast) · Esc
  closes overlays.
- IPC inputs validated at the boundary · no secrets in the bundle · user-facing strings
  i18n-routed.

## Milestone order (self-gate each; >3 files ⇒ plan first)

M0 scaffold (Next+Tauri+Tailwind, static export proven) → M1 shell + design-system
primitives + tokens → M2 Home + Chat + composer → M3 Butler Mode → M4 Code page → M5
Settings/Profiles/rest → M6 packaging + icon/font swap-in.

## Confirm at GATE before M0 (blocking)

1. Backend/IPC seam for Chat, Code, and Butler-voice — which existing surface
   (`regent-web` HTTP? `regent-cli`? `regent-voice-server`?), and Tauri Rust sidecar vs.
   existing daemon?
2. KONTES font file — source/license, or approve the fallback for now?
3. Dark mode — in scope for v1 or Light-only?
4. Placeholder logo/app-icon acceptable until real assets land?
