# ADR-014: TypeScript/Ink front-end (regent-cli) — the sole CLI plane; Bun single-binary

**Status:** Accepted — 2026-06-18. **Amended 2026-06-20:** the Go CLI was retired and removed, and
this front-end was renamed `src/regent-tui` → `src/regent-cli` — it is now the sole CLI plane
(superseding ADR-012). Decision point 1 below ("coexist, don't replace") is thus resolved: the
replacement happened.

**Context:** The brief calls to rebuild Regent's terminal front-end as an original TypeScript/Ink
implementation at reference-grade craft, without adding install friction. ADR-012 and next-steps.md
had committed to the Go CLI now and deferred TS Ink to P8. This ADR records the user's pivot to build
the Ink front-end now, with constraints that keep it from regressing the Go plane or the
zero-dependency install.

**Decision:**
1. **Coexist, don't replace (yet).** `src/regent-tui/` is a new, independent front-end alongside
   `src/regent-cli/` (Go). Both are thin JSON-RPC clients to `regent-deacon`; no Rust/Go code changes.
   Retiring the Go CLI is a later, separate decision once Ink reaches parity.
2. **Bun single-binary distribution.** Build with Bun; ship via `bun build --compile` as one
   self-contained executable per platform — the analog of Go's static binary, so the front-end adds
   no runtime dependency. Ink's dev-only `react-devtools-core` import is neutralised at build time
   with `--define process.env.DEV="false"` (the minifier prunes the dead branch).
3. **Reference reuse = adapt patterns onto npm `ink`.** The reference (a leading CLI agent's
   forked Ink renderer) is studied for craft and reimplemented on the published `ink` package, fitted to
   Regent's tree/palette — not vendored. Chosen for lowest IP exposure and a clean dependency
   (the entangled, proprietary fork pulls internal modules + a custom yoga build).
4. **Clean architecture, literally.** app/shared/features with the inward dependency rule; kernel
   holds Result + the `IRpcClient` contract; DI is the only composition site (ADR-007, canonical tree).
5. **Brand:** gradient-silver "REGENT" wordmark + kneeling-king mark; **gold crown** (per the
   canonical `Regent.psb`, correcting ADR-012's "teal crown"); teal #00A19B is the UI accent.

**Consequences:** Phase 1 ships the connect→welcome slice. Phases 2–4 add chat
streaming/approval/interrupt, the Go-parity subcommands, then animation/polish to reference craft.
The daemon JSON-RPC contract stays the single source of truth shared by the Go CLI, gateway, and the
Ink surface — they cannot drift. Supersedes the "TS deferred to P8 / optional Ink TUI" line in
ADR-012 and next-steps.md §P8.
