# Contributing to Regent

Contributions land as small, verified, atomic changes. This page is the whole process.

## Ground rules

1. **One atomic change per commit** — the smallest complete, working unit (one fix, one
   feature slice), with its tests in the same commit.
2. **Conventional Commits** — `feat:`, `fix:`, `refactor:`, `test:`, `docs:`, scoped when
   useful (`feat(cron): …`). Never force-push shared branches; never commit secrets
   (history is forever).
3. **Green before merge** — the crates you touched pass `cargo test -p <crate>` (plus
   `cargo clippy`), and CLI changes pass `tsc`, `biome`, and `bun test`. A lint pass alone
   is not verification.
4. **Never weaken a test to make it pass.** A failing test means fix the code — unless the
   test itself is provably wrong, in which case say so in the commit body.
5. **Decisions get ADRs** — anything that constrains future work (a library, a schema, a
   protocol) gets a ≤15-line record in `docs/adr/`, and `docs/changelogs/CHANGELOG.md` gets a dated
   entry saying what changed and how it was verified.

## Setup

Prereqs and build steps are in the [root README](../README.md#install) (Windows/macOS/Linux).
TL;DR: Rust 1.96+ (pinned by `rust-toolchain.toml`), Bun 1.x, and — only for the voice
server — LLVM/libclang.

```bash
cargo build --workspace                  # everything (voice server needs LLVM)
cargo test  --workspace                  # full suite
cd src/regent-cli
bun install
bun run typecheck; bun run lint; bun test
bun run install-cli                      # optional: put your dev `regent` on PATH
```

## Where things go

| You're adding… | It goes in… |
|---|---|
| A tool the agent can call | `regent-tools/src/infra/<tool>.rs` + register in `application/registry.rs` (definition = what the model sees; executor = what runs). Rare/heavy tools also join the `tools.deferred` default list |
| A chat platform | `regent-deacon/src/infra/webhook.rs` registry entry + an adapter next to the existing 17 (`verify_request` → parse → reply). Add its secrets to `regent keys`'s MANAGED lists (TS + Rust) and the QUICKSTART matrix |
| A model provider | `regent-providers` (OpenAI-compatible hosts are usually just a base URL in `regent-deacon`'s `provider_factory.rs`) |
| An agent behavior/prompt change | `regent-agent/src/domain/prompts.rs` — keep the cached prefix byte-stable; run the memory/eval tests |
| A CLI command | `src/regent-cli/src/features/<feature>/cli/` + router + `help.ts` (help is generated from one map) |
| A bundled skill | `skills/<name>/SKILL.md` (repo) + the `include_str!` list in the deacon's `seed_bundled_skills` — seeds to `~/.regent/skills` at boot, never overwriting user edits |
| Docs | `docs/` per the [folder map](../docs/README.md#folder-map); user-visible changes also update `docs/changelogs/CHANGELOG.md` |

**Architecture invariants** (enforced in review): every crate keeps
`domain / application / infra` layering with dependencies pointing inward; tools never
bypass domain logic; retrieved/web content is data, never instructions; secrets never
reach logs (the redacting writer) or `config.yaml`.

## Sending a contribution

Fork → branch (`feat/<slug>`) → atomic commits → open a PR describing **what changed,
why, and the exact commands that verified it** (paste the test summary). Small PRs merge
fast; a PR that mixes refactors with features will be asked to split.

Drop larger proposals as a design doc in `docs/proposal/` first (see existing ones for the
shape) — agreeing on the plan before the code saves everyone a rewrite.

This folder (`contributions/`) also hosts contributed assets that aren't code —
benchmarks, eval sets, platform configs, skill packs. Add a subfolder with a README
saying what it is and how it was produced.
