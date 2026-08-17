<p align="center">
  <img src="assets/Regent_Image_Banner.png" alt="REGENT" width="100%" />
</p>

# Regent ⚚
<p>
  <img alt="license" src="https://img.shields.io/badge/license-MIT-brightgreen" />
  <img alt="built by" src="https://img.shields.io/badge/built%20by-Regent33-6b21a8" />
  <img alt="platform" src="https://img.shields.io/badge/runs%20on-Windows%20%7C%20macOS%20%7C%20Linux-0b8782" />
  <img alt="local" src="https://img.shields.io/badge/works-fully%20offline%20with%20Ollama-19b3ac" />
  <a href="https://discord.gg/dM3ZjUbgHs"><img alt="discord" src="https://img.shields.io/badge/discord-join%20the%20community-5865F2?logo=discord&logoColor=white" /></a>
</p>

**Your own AI assistant, living on your computer — not in someone else's cloud.**
It chats in your terminal, talks on live voice calls (and *sees* — your camera and your
screen), messages you on 17+ platforms, fixes code without breaking your project,
remembers everything it learns, and builds real documents. Rust core, single-binary CLI,
your keys and data never leave your machine.

Use any model you want — Anthropic, OpenRouter, OpenAI-compatible hosts, or fully
offline with [Ollama](https://ollama.com). Switch with `regent model` — no code changes,
no lock-in.

| | |
|---|---|
| **A real terminal interface** | Full TUI with streaming replies, slash commands, session history, interrupt/redirect, and approval prompts for anything risky. |
| **Lives where you do** | Telegram, Slack, Discord, WhatsApp, Messenger, LINE, Teams, Google Chat, WeChat, WeCom, Feishu, Twilio SMS/voice, email, Jira, Azure DevOps, Trello, Mattermost — signature-verified, sandboxed by default. |
| **Voice calls with vision** | `regent call` — speech runs locally (whisper + Kokoro). Ask *"are you seeing what I'm seeing?"* (screen) or *"what am I holding?"* (camera). |
| **Careful coding** | `regent code "<task>"` (or just ask in chat) plans first, edits, runs **your repo's own tests**, and reverts everything if they fail. |
| **A closed learning loop** | Tri-modal memory (keyword + semantic + graph), episode capture across sessions, self-authored SKILL.md playbooks, and a curator that archives stale agent-created playbooks without deleting them. |
| **Scheduled automations** | Cron jobs in natural language that survive reboots — daily reports, backups, reminders, delivered to any connected platform. |
| **Real documents** | The bundled `documents` skill uses native tools to read and create PowerPoint, Word, Excel, and PDF files — not markdown dumps. |
| **Safe by default** | External messages run filesystem-jailed; their memory writes wait for your approval; dangerous commands stop and ask; secrets live in one owner-only file, masked in every log. |
| **Research-ready** | Eval-gated memory retrieval (recall@5 ≥ 0.75), documented architecture decisions, reproducible test suites per crate, full audit trail in [docs/](docs/README.md). |

## Quick Install

### Linux, macOS

```bash
curl -fsSL https://raw.githubusercontent.com/Regent33/Regent/main/scripts/install.sh | sh
```

### Windows PowerShell

```powershell
irm https://raw.githubusercontent.com/Regent33/Regent/main/scripts/install.ps1 | iex
```

### Windows Command Prompt (`cmd.exe`)

```bat
powershell -NoProfile -ExecutionPolicy Bypass -Command "iex (irm 'https://raw.githubusercontent.com/Regent33/Regent/main/scripts/install.ps1')"
```

The script downloads the latest release plus its SHA-256 file and verifies it
before extraction; if no verified asset exists, it offers the existing
build-from-source path. In an interactive terminal it opens `regent setup`
automatically. Pick **ollama** there to run locally with no API key.

| Install path | Default location |
|---|---|
| One-line CLI, Windows | `%USERPROFILE%\.regent\bin` |
| One-line CLI, macOS/Linux | `~/.regent/bin` (`regent` link in `~/.local/bin`) |
| GUI Setup, Windows | `%LOCALAPPDATA%\Programs\Regent` |
| GUI Setup, Linux | `~/.local/share/Regent` |

GUI installers for Windows and Linux are on
[GitHub Releases](https://github.com/Regent33/Regent/releases/latest). The macOS
GUI waits for Apple Developer ID signing; use the verified one-line installer meanwhile.

On macOS, Regent needs **Apple Silicon**. Its embedding engine is ONNX Runtime,
which no longer publishes an x86_64 macOS build, so Intel Macs cannot be
supported — building from source hits the same wall. Windows and Linux are
x86_64.

<details>
<summary><b>Build from source instead</b> (Rust 1.96+ and Bun)</summary>

```bash
git clone https://github.com/Regent33/Regent && cd Regent
cargo build --release -p regent-deacon
cd src/regent-cli
bun install
bun run install-cli     # compiles + puts `regent` on your PATH
```

Choose your own install dir: `bun run link -- --dir <path>` (default:
`%USERPROFILE%\.bun\bin` on Windows, `~/.local/bin` elsewhere).
Voice calls additionally need LLVM/libclang —
see [docs/development/](docs/development/voice-and-api-calls.md).
</details>

**What gets downloaded.** The one-line installer fetches the CLI + deacon
archive and its checksum. Windows may also install a pinned, hash-verified
ffmpeg build for optional camera capture; macOS/Linux print the package-manager
command instead. Memory and local speech models download only when first used.
There is no telemetry.

**Uninstall the matching install type:**

```bash
# macOS/Linux one-line install
curl -fsSL https://raw.githubusercontent.com/Regent33/Regent/main/scripts/uninstall.sh | sh
```

```powershell
# Windows PowerShell one-line install
irm https://raw.githubusercontent.com/Regent33/Regent/main/scripts/uninstall.ps1 | iex
```

```bat
:: Windows cmd.exe one-line install
powershell -NoProfile -ExecutionPolicy Bypass -Command "iex (irm 'https://raw.githubusercontent.com/Regent33/Regent/main/scripts/uninstall.ps1')"
```

These keep config, keys, sessions, and memory unless purge is explicitly set.
For the GUI app, use **Installed apps → Regent → Uninstall** (or rerun Linux
Setup); that removes the app, CLI/deacon payload, shortcuts, PATH entry, and
deacon pin while preserving user data.

## Getting Started

```bash
regent                  # interactive CLI — start a conversation
regent call             # live voice call (camera + screen vision)
regent code "<task>"    # plan → edit → test → revert-if-broken coding
regent model            # choose your LLM provider and model
regent cron add …       # schedule automations in natural language
regent gateway          # start the messaging gateway (Telegram etc.)
regent migrate hermes   # import an existing Hermes install (skills & more)
regent doctor           # diagnose exact provider/key routing and install health
regent help             # everything else
```

Every command also works inside chat as `/command`.

Use `/with <provider>/<model> <task>` for a one-turn model override without
changing the app's main model.

## Documentation

All documentation lives in [docs/](docs/README.md):

| Section | What's covered |
|---|---|
| [Quickstart](docs/QUICKSTART.md) | install → provider → chat → platforms → sandboxing |
| [Commands](docs/reference/commands.md) | every command, annotated |
| [Environment variables](docs/reference/env-vars.md) | every knob, reconciled against the code |
| [Development](docs/development/README.md) | building & testing per toolchain and OS |
| [Architecture decisions](docs/adr/) | Why things are the way they are |
| [Changelog](docs/changelogs/CHANGELOG.md) | what changed, when, and how it was verified |

## Migrating from Hermes or OpenClaw

```bash
regent migrate hermes/openclaw     # dry-run by default — shows what would be imported
regent migrate hermes --apply
```

Skills import today (agentskills.io format copies as-is); the source install is never
touched, and existing Regent skills are never overwritten.

## Community

Join the [Discord](https://discord.gg/dM3ZjUbgHs) — help, showcase what you built,
bug reports, and dev logs as things ship.

## Contributing

Contributions are welcome — see the [Contributing Guide](contributions/README.md) for
setup, code style (domain/application/infra layering), the ADR process, and what goes
where. Small, verified, atomic PRs merge fast.

## License

MIT.

Built by **Regent33 / Rainer Lacanlale**.
