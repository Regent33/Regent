# Regent documentation

Start here. Each section stands alone — read only what you need.

## If you want to…

| Goal | Read |
|---|---|
| **Install and run Regent** (any OS) | [../README.md](../README.md) → [QUICKSTART.md](QUICKSTART.md) |
| **Get the 5-minute architecture tour** | [PROJECT-OVERVIEW.md](PROJECT-OVERVIEW.md) |
| **Look up a command** | [reference/commands.md](reference/commands.md) |
| **Look up an env var / secret** | [reference/env-vars.md](reference/env-vars.md) |
| **Wire a chat platform** (Telegram, Slack, WhatsApp, …) | [QUICKSTART §6](QUICKSTART.md#6-messaging-platforms) |
| **Understand the architecture** | [../README.md §Architecture](../README.md), then [adr/](adr/) in order |
| **Build / test / hack on it** | [development/](development/) + [../contributions/README.md](../contributions/README.md) |
| **See what changed and how it was verified** | [changelogs/CHANGELOG.md](changelogs/CHANGELOG.md) |
| **Audit its security posture** | [audits/](audits/) (scans) + ADR-030/031 + [QUICKSTART §Sandboxing](QUICKSTART.md#sandboxing-tool-execution) |
| **Read the forward plans** | [plans/](plans/) |

## For researchers

Regent is a local-first personal agent: Rust/Tokio core (`regent-deacon`) driven over
JSON-RPC by a TypeScript/Ink CLI, with tri-modal graph memory (FTS5 + vector + graph,
eval-gated: recall@5 ≥ 0.75 enforced in CI tests), a self-learning SKILL.md library, a
verify-and-revert coding harness, signature-verified webhook adapters for 17 platforms,
and a fully local voice stack (sherpa-onnx) with screen+camera vision.

Suggested reading order:

1. [../README.md](../README.md) — what it is, repo map
2. [adr/](adr/) — 36 decision records; ADR-001/002 (runtime), ADR-006/013 (memory),
   ADR-027 (coding harness), ADR-028 (constitution), ADR-029 (voice), ADR-030–032 (security,
   token efficiency, vendoring)
3. [audits/](audits/) — the 2026-07-02 full scan and the remediation it drove
   (see the CHANGELOG entry of the same date for what shipped)

**Credits / lineage:** before this rebuild we studied
[NousResearch's hermes-agent](https://github.com/NousResearch/hermes-agent) in
depth — how it works, how its pieces interconnect, and where its gaps were —
and built Regent as our own independent, improved implementation of those
ideas (Rust core, tri-modal memory, verify-and-revert coding, local voice).
The detailed study notes are internal working documents and aren't shipped in
this repo.

Reproducibility: every crate tests with `cargo test -p <crate>`; the CLI with `bun test` +
`tsc` in `src/regent-cli`; memory quality gates live in `regent-graph/tests/golden_retrieval.rs`
and `regent-embed/tests/fusion_eval.rs`.

## Folder map

| Folder | Contents |
|---|---|
| `adr/` | Architecture Decision Records (numbered, ≤1 page each) |
| `audits/` | security/robustness scan reports |
| `development/` | per-toolchain build/test guides (Rust, TS CLI, voice, desktop) |
| `others/` | deep dives that fit no other folder (sandboxing, memory retrieval, daemon design, …) |
| `plans/` | forward-looking plans & designs (including executed ones, kept for the record) |
| `reference/` | flat lookup tables: commands, env vars |
| `changelogs/CHANGELOG.md` | dated, verified change log |
| `QUICKSTART.md` | zero-to-chatting walkthrough |
