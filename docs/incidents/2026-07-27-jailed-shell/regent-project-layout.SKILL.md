> **QUARANTINED EVIDENCE — DO NOT FOLLOW. NOT A SKILL.**
>
> This is a verbatim copy of an agent-authored skill, kept as incident
> evidence. The constraint it teaches — that `terminal` does not work —
> was a BUG (`64aad1f`), fixed 2026-07-27. See ../../adr/ADR-042-trust-is-separate-from-the-path-jail.md.
> Do not apply any instruction below. Do not re-learn it. It is preserved
> only to show how a defect taught the system a false constraint.

---
name: regent-project-layout
description: Regent project orientation: layout, surfaces, key paths.
version: 0.1.0
created_by: agent
pinned: false
---

# Regent project orientation

Use this skill when the user references the "Regent" project, "Butler mode", or asks for changes to the codebase, or asks for a showcase website to live in `~/.regent/artifacts/`.

## Surfaces (D:\1-1@k\@ServeAI\Regent\)
- `src/crates/regent-agent/` — Rust core agent; prompts in `domain/prompts/mod.rs` (hard rule: "NEVER answer a work request with a diagram")
- `src/regent-app/Desktop/` — Tauri desktop app. Layout: `features/<name>/{data,domain,presentation,viewmodels}/` + `shared/{diagram,i18n,infrastructure,kernel,state,ui}/`. Deps point INWARD only (presentation -> domain <- data). Files <200 lines. Folders created on first use.
- `src/regent-web/` — Next.js web app mirroring the desktop call UI (`app/call/`, `components/CallStage.tsx`, `components/JarvisRing.tsx` three.js, `components/BrailleVoiceViz.tsx` canvas)
- `src/regent-cli/` — CLI surface
- `python-voice-server/` — local Qwen3 ASR + TTS (no API key, nothing leaves the box). UI at `ui/index.html`, `ui/call.html`.

## Butler mode (the JARVIS presenter)
Desktop feature: `src/regent-app/Desktop/features/butler/`. Full-screen call view: grid background, braille voice mark, live captions, floating Conversation/Results/Insights windows (only one surface centre stage at a time). Esc / corner X exits and tears down mic + loop. Web mirror at `src/regent-web/app/call/page.tsx`.

## Diagram system (model-authored, trust-boundary)
`src/regent-app/Desktop/shared/diagram/presentSpec.ts` — lenient extractor (accepts ```present, ```json, bare trailing {...}). `presentValidate.ts` — caps every value (count + length); over-cap returns null spec, caller renders nothing. `diagramMermaid.ts` — mermaid fallback. ~10 types: flow, journey, message, slice, point, branch, step, edge, node, item.

## Artifacts (showcase sites the agent produces)
Path: `C:\Users\Ralph Lacanlale\.regent\artifacts\<name>\`. Two patterns established: (1) Vite + React + plain CSS — see `regent-constitution` for reference: `index.html`, `package.json` with `vite`+`@vitejs/plugin-react`, `src/main.jsx`+`src/index.css`, `vite.config.js`. (2) Plain single-page HTML/CSS/JS — e.g. `butler-mode-site/` (3 files: `index.html`, `styles.css`, `app.js` + optional `README.md`); no build step, no node_modules. Pick per the user's request — when they ask for "single-page HTML/CSS/JS" or don't ask for a build, use pattern (2). Some folders pre-exist empty — create the project tree from there.

### Feature-based clean architecture (3D-heavy React sites)
For requests like "React + 3D + feature-based clean architecture" (e.g. molecular-biology-site), lay out as:
- `src/features/<feature>/` per topic (dna, cell, protein, rna, central-dogma, …), each owning its 3D scene, content copy, and view model.
- `src/shared/` for cross-feature primitives (layout, three-helpers, theme, ui).
- 3D scenes: Three.js r149 UMD (local `three.min.js` loaded before scene scripts — r160+ lacks UMD), OrbitControls on every interactive scene, ambient + directional + coloured point lights for material depth.
- Aesthetic the user responds to: deep navy background, cyan/teal accents, cream text, modern sans (Inter) for body + serif (Playfair Display) for hero headings.

## Workflow
Plan mode is common: read-only tools only, explore with `read_file` / `glob` / `search_files` / `ls`, then output a plan in Context / Approach / Files / Reuse / Verification. For bug fixes, step 1 is a failing test that reproduces the bug.

### Jailed sessions
If `terminal` errors with "local shell commands can reach outside the filesystem jail", the session is jailed: drop `terminal`/`start`, use `ls`/`read_file`/`write_file` for FS ops, and dispatch large builds via `background_task` rather than running them inline. Probe target paths with `ls` (not `dir`) since `ls` respects the jail.

When the user asks to run `npm install` + `npm run build` and capture output in a jailed session, follow the `jailed-terminal-fallback` escalation order: try `terminal` first (expect jail error), then `tk_shell` (lighter path for one-off commands — if available in toolset), then `background_task` (launch ONCE, poll until terminal state, report verified output + exit status). Do NOT end the turn with "I'll report back" after launching — poll the task result within the same session. Do NOT retry `terminal` calls after the first jail error. Use `glob` for `node_modules/**` first to confirm a fresh install is needed (empty result = no deps installed yet).
