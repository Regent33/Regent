# Regent — Project Overview

Regent is a personal AI agent that lives on your computer. You talk to it in a
terminal, on a live voice call, in a desktop app, or from chat apps like Telegram
and Slack. It remembers what it learns, can edit code without breaking your
project, and runs scheduled jobs while you sleep. Your API keys and data stay on
your machine.

This page explains how it's built and why — written for someone seeing the repo
for the first time. For install-and-run, start at the [root README](../README.md)
and [QUICKSTART](QUICKSTART.md) instead.

## The one-paragraph architecture

Everything intelligent lives in one long-lived Rust process, **regent-deacon**
(the "daemon", renamed to deacon). Every user interface — the terminal CLI, the
desktop app, the voice server, the messaging gateway — is a thin client that
talks to the deacon over JSON-RPC 2.0. The deacon owns sessions, memory, tools,
cron, and model calls; the front-ends just render. That's the whole trick: fix a
behavior once in Rust and every surface gets it.

```
 you ──┬─ regent-cli   (terminal, TypeScript/Ink, one ~100 MB binary)
       ├─ Regent Desktop (Tauri + React, src/regent-app/Desktop)
       ├─ regent-voice-server (live calls: whisper ASR + Kokoro TTS, local)
       └─ regent-gateway (Telegram, Slack, Discord, WhatsApp, … 17 platforms)
                │
                ▼  JSON-RPC 2.0 over stdio
         regent-deacon  ← the only process that thinks
                │
    ┌───────────┼──────────────┐
    ▼           ▼              ▼
 LLM providers  SQLite store   tools & skills
 (Anthropic,    (sessions,     (terminal, files,
  OpenRouter,    tri-modal      search, coding
  Ollama, any    memory)        harness, docs)
  OpenAI-style)
```

## What's in the repo

| Path | What it is |
|---|---|
| `src/crates/` | The Rust core — 17 crates, described below |
| `src/regent-cli/` | The terminal front-end (TypeScript + Ink, compiled by Bun into a single binary) |
| `src/regent-app/Desktop/` | The desktop app (Tauri 2 + React 19 + Vite) |
| `src/regent-web/` | A small Next.js page used by `regent call` for the browser call UI |
| `python-voice-server/` | The original Python voice server — kept as a fallback; the Rust port is the real one |
| `scripts/` | One-line installers (`install.sh`, `install.ps1`) |
| `docs/` | Everything else: quickstart, command reference, 36 ADRs, changelog |

The crates that matter most on a first read:

- **regent-deacon** — the composition root. Loads config, owns sessions, speaks JSON-RPC.
- **regent-agent** — the turn loop: budgets, interrupts, parallel tool dispatch.
- **regent-kernel** — shared contracts: messages, transcript invariants, secret redaction.
- **regent-providers** — LLM providers with native tool calling (Anthropic, OpenAI-style, Ollama).
- **regent-tools** — the tool catalog plus the dangerous-command guard.
- **regent-code** — the coding harness: plan → edit → run your repo's tests → revert if broken.
- **regent-store / regent-graph / regent-embed** — the three lanes of memory: SQLite FTS5
  keyword search, a provenance-tagged knowledge graph, and local ONNX embeddings.
- **regent-skills** — self-authored SKILL.md playbooks with usage telemetry and a curator
  that prunes stale ones.
- **regent-gateway** — platform adapters, signature verification, pairing-based auth.
- **regent-cron** — scheduled jobs with catch-up clamps and hard timeouts.
- **regent-speech / regent-realtime / regent-voice-server** — the voice stack (details below).

## Design decisions, and why

Each of these has a full ADR in [docs/adr/](adr/); this is the short version.

**One Rust core, thin clients (ADR-001, ADR-033).** The predecessor system
(Hermes, studied in [hermes-study/](hermes-study/)) mixed orchestration into its
UIs and paid for it in drift. Regent's rule: if it makes a decision, it lives in
the deacon. The CLI and desktop app contain no agent logic at all.

**Vendored orchestration (ADR-002, ADR-032).** The `or-core`/`or-mcp` libraries
are vendored in-repo at `src/crates/regent-orchustr-core` — no surprise upstream
breakage, and native (OpenAI-style) tool calls instead of prompt-parsed ones.

**SQLite for everything (ADR-003).** Sessions, messages, memory, cron state —
one WAL-mode SQLite file under `~/.regent`. No server to run, trivially backed
up, and FTS5 gives keyword search for free.

**Memory is tri-modal and eval-gated (ADR-006, ADR-013).** Keyword (FTS5),
semantic (local MiniLM embeddings — no embedding API bill), and graph (typed
nodes/edges with provenance). Retrieval quality is enforced by tests:
recall@5 ≥ 0.75 in `regent-graph/tests/golden_retrieval.rs`. If a change makes
memory worse, CI fails.

**Coding reverts itself (ADR-027).** `regent code "<task>"` writes a plan you
approve, edits, then runs *your repo's own* test command after each step. A red
step reverts to the last green state. The agent never leaves your repo broken.

**Speech is local (ADR-029).** ASR is whisper, TTS is Kokoro, both running
through sherpa-onnx on your CPU — a live call sends audio nowhere. The trade-off:
building the voice server needs LLVM/libclang, so it's an optional component,
disabled by default.

**Safety is layered, not vibes (ADR-030).** Messages arriving from external
platforms run filesystem-jailed; their memory writes queue for your approval.
Dangerous terminal commands stop and ask. Secrets live in one owner-only `.env`
file (0600), and every log line passes through a redactor that masks known key
prefixes (`sk-ant-…`, `xoxb-…`, `ghp_…`) before it hits disk.

**Config is data, secrets are not config.** `~/.regent/config.yaml` holds
behavior (provider, model, tool settings); `~/.regent/.env` holds keys. Nothing
secret ever goes in YAML, so config can be shared and diffed freely.

## What it does (with the command that does it)

```bash
regent                    # chat in your terminal — streaming, slash commands, history
regent call               # live voice call; it can see your screen or camera on request
regent code "fix the flaky retry test"   # plan → edit → test → revert-if-broken
regent model              # switch providers/models — Anthropic, OpenRouter, Ollama, …
regent gateway setup <token>   # connect Telegram (and 16 more platforms)
regent cron add "every morning at 8, summarize my inbox"
regent memory pending     # review what external chats want to remember
regent migrate hermes     # import skills from a Hermes install (dry-run by default)
regent doctor             # when anything misbehaves, start here
```

Every command also works inside chat as `/command`. The desktop app exposes the
same sessions and settings — same deacon, different window.

## Known edges (honest list, as of 2026-07)

- **Voice build needs LLVM.** `cargo build -p regent-voice-server` fails without
  libclang; see [development/voice-and-api-calls.md](development/voice-and-api-calls.md).
  Prebuilt releases will bundle it; source builds must install it.
- **The voice server binds one deacon for its lifetime.** Restarting the deacon
  mid-call requires restarting the voice server too.
- **The CLI binary is big (~100 MB).** That's the cost of Bun single-binary
  packaging: the runtime is embedded. It starts fast; it just isn't small.
- **Desktop app is young.** Chat, settings, Butler (voice) mode, and session
  history work; expect rough edges everywhere else. It requires a locally built
  deacon today.
- **Windows is the primary dev platform.** macOS/Linux paths are exercised by
  installers and CI, but day-to-day development happens on Windows — report
  platform quirks, they're bugs, not choices.
- **`regent-web` is single-purpose.** It's the browser page `regent call` opens,
  not a general web UI. A real web surface may come later.
- **No auto-update yet.** Re-run the installer to upgrade.

## Where to go next

- Hack on it: [contributions/README.md](../contributions/README.md) — layering
  rules, ADR process, what goes where.
- Understand a subsystem: [adr/](adr/) — 36 records, each ≤ 1 page.
- See what changed and how it was verified: [CHANGELOG.md](CHANGELOG.md).
