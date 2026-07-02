# Reference — Commands

Every `regent` command, from the CLI's own help map (`src/regent-cli/src/app/cli/help.ts`
is the source of truth; run `regent help` for the live list). All commands also work
**in-chat** prefixed with `/` (e.g. `/status`, `/kanban list`). Global flag:
`-p, --profile <name>` isolates state under a profile home.

## Daily use

| Command | What it does |
|---|---|
| `regent` / `regent chat` | interactive streaming chat (default) |
| `regent code "<task>"` | plan-mode coding: plan → verify → revert (`--yes` auto-approves the plan). In chat, the agent routes nontrivial code changes here itself via the `code_task` tool |
| `regent call` | live real-time voice call (local ASR/TTS; screen + camera vision) |
| `regent sessions` | list · search · resume past sessions |
| `regent status` | deacon health / model / cron snapshot |

## Memory & learning

| Command | What it does |
|---|---|
| `regent memory` | pending · approve · reject · list · pin · unpin · forget — pending holds writes proposed by external (platform) sessions |
| `regent skills` | list · view · create · opt-out (SKILL.md library; `doc-forge` ships bundled) |
| `regent persona` / `soul` / `about` | view/edit the agent persona and your user profile |
| `regent insights` | usage rollup (turns, tokens, api calls) |

## Models & providers

| Command | What it does |
|---|---|
| `regent setup` | first-time configuration (provider, model, key; `--provider ollama` = local, keyless) |
| `regent model` | show · list · set `<id>` |
| `regent providers` | list · add · remove · test model providers |
| `regent keys` | manage provider/platform keys in `.env` (list · set · rm) — owner-only file perms |

## Automation & platforms

| Command | What it does |
|---|---|
| `regent cron` | list · add · remove · pause · resume · run · edit · **autostart** (fire after reboot with no session open) |
| `regent kanban` | task board: list · create · show · assign · start · review · block · complete |
| `regent agents` | named agents + `mom` Mixture-of-Models groups |
| `regent gateway` | Telegram long-poll plane: setup `<token>` · start · stop · status |
| `regent auth` | platform pairing status · revoke `<user>` |
| `regent mcp serve` | expose Regent's tools over MCP (stdio) |

## Operations

| Command | What it does |
|---|---|
| `regent doctor` | check the installation (toolchain, db, provider reachability) |
| `regent logs` | show the redacted deacon log (`-f` to follow) |
| `regent security` | audit permissions / secrets |
| `regent debug` | redacted bug-report bundle |
| `regent config` | show · set `<key> <value>` (config.yaml) |
| `regent tools` | list · enable · disable `<tool>` (see also config `tools.deferred`) |
| `regent profile` | list · create · delete profile homes |
| `regent migrate` | import a Hermes/OpenClaw install (dry-run by default; `--apply` to write) |
| `regent version` / `help` | version · full annotated command list |

## In-chat only

`/help` · `/doctor` · `/new` (clear transcript) · `/stop` (interrupt turn) ·
`/approve` / `/deny` (pending sensitive action) · `/quit`
