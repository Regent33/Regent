# Regent — Quickstart

Get Regent running, connect a model, and (optionally) wire a chat platform.

> **Shortcut:** skip the build entirely with a verified release installer.

### One-line install

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/Regent33/Regent/main/scripts/install.sh | sh
```

```powershell
# Windows PowerShell
irm https://raw.githubusercontent.com/Regent33/Regent/main/scripts/install.ps1 | iex
```

```bat
:: Windows Command Prompt (cmd.exe)
powershell -NoProfile -ExecutionPolicy Bypass -Command "iex (irm 'https://raw.githubusercontent.com/Regent33/Regent/main/scripts/install.ps1')"
```

The release archive is checked against its published SHA-256 before extraction.
Interactive one-line installs open `regent setup` automatically. GUI installers
for Windows and Linux are on [GitHub Releases](https://github.com/Regent33/Regent/releases/latest);
macOS stays on the one-line path until its GUI build can be Developer-ID signed.

| Installer | Default location |
|---|---|
| Windows one-line CLI | `%USERPROFILE%\.regent\bin` |
| macOS/Linux one-line CLI | `~/.regent/bin` + `~/.local/bin/regent` |
| Windows GUI Setup | `%LOCALAPPDATA%\Programs\Regent` |
| Linux GUI Setup | `~/.local/share/Regent` |

### Check for updates

```bash
regent update           # show the deacon's cached official release status
regent update --check   # explicit status alias
```

The deacon is the one bounded background checker; this command reads its cached
verdict and clearly distinguishes the CLI version, deacon version, newest known
release, and a mixed installation. It never downloads or replaces binaries.
Use the verified platform installer linked in its output when an update is
available. Safe in-place apply still requires Regent's planned versioned launcher,
durable switch journal, compatibility checks, and database backup/rollback.

## 1. Build

```bash
# Rust core (self-contained — or-core/or-mcp are vendored in-repo)
cargo build --release -p regent-deacon
# CLI (TypeScript/Ink, compiled to a single self-contained binary with Bun).
# One command per line — works in PowerShell (no `&&`) and bash alike.
cd src/regent-cli
bun install
bun run install-cli
```

This produces `target/release/regent-deacon` (+ `regent-mcp`) and `src/regent-cli/dist/regent-cli`,
then installs a `regent` launcher on your PATH. The CLI locates the daemon via
`REGENT_DEACON_PATH`, a sibling binary, `PATH`, or the cargo `target/` dir — so a dev build is
found automatically. (During development you can skip all of this and run `bun run dev`.)

### Where the `regent` command lands (and how to choose)

`bun run install-cli` (= `compile` + `link`) installs a launcher at the **default** location:

- **Windows:** `%USERPROFILE%\.bun\bin\regent.cmd` — a `.cmd` shim; that dir is already on PATH
  from installing Bun.
- **macOS/Linux:** `~/.local/bin/regent` — a symlink.

**Choose your own directory** either way:

```bash
bun run link -- --dir /usr/local/bin        # explicit flag (any OS)
REGENT_LINK_DIR=~/bin bun run link          # or via env
```

The launcher points at `dist/regent-cli(.exe)`, so after CLI code changes a plain
`bun run compile` refreshes what `regent` runs — no re-link needed. If your chosen dir isn't
on PATH the script says so and prints the fix. Verify with `regent --version` and `regent doctor`.

### Uninstall

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

The one-line uninstallers stop Regent, remove their CLI/deacon binaries and
link/PATH entry, and keep data unless purge is explicit. GUI installs use
**Installed apps → Regent → Uninstall** on Windows or rerun Linux Setup; the GUI
uninstaller also removes the app, shortcuts, deacon pin, and registration.
Dev installs via `bun run install-cli` only need the printed launcher removed.

## 2. First-time setup

An interactive one-line install starts this automatically. Run it yourself if
the installer had no terminal or `REGENT_NO_LAUNCH=1` was set:

```bash
regent setup            # interactive: provider, model, API key when required
# or non-interactive:
printf %s "$ANTHROPIC_API_KEY" | regent setup --provider anthropic --model claude-sonnet-4-6 --key-stdin
regent setup --provider lmstudio --model local-model --base-url http://localhost:1234
```

PowerShell: copy the key, then use
`Get-Clipboard | regent setup --provider anthropic --model claude-sonnet-4-6 --key-stdin`.
Secrets are never accepted as command-line arguments, where shell history and
process listings could expose them.

`setup` writes two files under `$REGENT_HOME` (default `~/.regent`):

- **`.env`** — secrets only (`REGENT_API_KEY`), written `0600` via an atomic, owner-only
  create (never briefly world-readable). The directory is tightened to `0700`.
- **`config.yaml`** — behavior (provider, model). Never holds secrets.

The CLI loads `$REGENT_HOME/.env` when it spawns the daemon; an explicit shell `export` always
wins over the file.

> **Secrets model:** plaintext files locked down by OS permissions (the Hermes approach), plus
> redaction at the logging boundary (a leaked key in a provider error body is masked before it
> reaches a log file). No secret is ever written to `config.yaml` or the repo.

## 3. Verify & chat

```bash
regent doctor           # install health, exact provider/key routing, config lint
regent chat             # interactive streaming chat (or just `regent`)
regent logs --follow    # tail the redacted rolling log
```

Inside chat, route one task without changing the main model:

```text
/with openrouter/anthropic/claude-sonnet-4.6 review this design
```

The first segment is the configured provider name. The remainder is that
provider's complete model id, including any vendor prefix it requires.

In the Desktop app, **Next request estimate** is the estimated context fill for
the next model call, not cumulative spend. **Reported turn input/output** totals
provider-reported usage across the whole turn, while **Reported last request**
shows only the final call. Regent flags calls whose provider omitted usage;
historical coverage from before store schema v11 is marked unknown rather than
shown as an exact zero.

## 4. Providers

`agents_defaults.primary` selects the provider/model route and resolves its
provider name through `providers.<name>`. `regent setup --provider ...` writes
both entries and keeps the legacy `model:` fields only for compatibility.
Anthropic uses the native Messages API; the rest are OpenAI-compatible and
differ primarily by base URL:

| provider | default host | notes |
|---|---|---|
| `anthropic` | api.anthropic.com | native, prompt-cache breakpoints |
| `openai` | openrouter.ai | historical default |
| `openrouter` | openrouter.ai | hundreds of models, one key |
| `groq` | api.groq.com | fast hosted open models |
| `deepseek` | api.deepseek.com | |
| `together` | api.together.xyz | |
| `ollama-cloud` | ollama.com | hosted Ollama, key required |
| `ollama` | localhost:11434 | local, key optional |
| `lmstudio` | localhost:1234 | local/self-hosted, key optional |
| `llamacpp` | localhost:8080 | local/self-hosted, key optional |
| `vllm` | localhost:8000 | local/self-hosted, key optional |
| `litellm` | localhost:4000 | local/self-hosted proxy, key optional |

Regent also has first-class defaults for Mistral, Gemini, NVIDIA NIM, xAI,
Moonshot, Fireworks, Cerebras, Perplexity, GitHub Models, and the other choices
shown by `regent setup`. Any other OpenAI-compatible host works via a configured
provider plus `base_url`.

In the Desktop app, **API Keys** contains separate **Image generation
providers** and **Video generation providers** sections alongside LLM,
local/self-hosted, search, speech, and vision/video-analysis credentials. The
same image/video keys work from the CLI:

```powershell
regent keys list
# PowerShell: copy one key, then pipe it without exposing it in shell history
Get-Clipboard | regent keys set STABILITY_API_KEY --stdin
Get-Clipboard | regent keys set RUNWAYML_API_SECRET --stdin
```

These sections store credentials; they do not pretend every vendor uses the
same API. Where a vendor publishes an environment-variable convention, Regent
uses it; otherwise the row is clearly a Regent-managed credential slot. Regent
currently ships vision/video *analysis* and one generic
OpenAI-compatible `image_generation` adapter configured with `REGENT_IMAGE_*`.
Native Stability/Runway/etc. generation adapters and native video generation
are still future work. Messaging platform credentials live only under
**Gateway**.

**Every command that takes a secret reads it from a pipe, never from the
command line.** A token typed as an argument is written to your shell's history
file and is readable in `ps`/Task Manager before Regent even starts, so those
argument forms are refused rather than warned about — the refusal tells you the
flag to use instead:

```powershell
Get-Clipboard | regent keys set ANTHROPIC_API_KEY --stdin
Get-Clipboard | regent setup --provider anthropic --model claude-sonnet-4-6 --key-stdin
Get-Clipboard | regent gateway setup telegram --token-stdin
Get-Clipboard | regent voice setup --provider elevenlabs --key-stdin
```

The flag is named for the secret each command wants: `keys set` already names
the key, so it is just `--stdin`; the others say which one they are reading.

## 5. Expose Regent's tools over MCP

```bash
regent mcp serve        # MCP server over stdio (point an MCP client at this)
```

Exposes the core tools + memory + skills with approval denied by default. stdout is the MCP
JSON-RPC stream; logs go to stderr.

## 6. Messaging platforms

Each platform normalizes its wire format to a shared message event behind a `WebhookAdapter`
(verify signature → parse → reply). The **verify/parse/build logic is implemented and unit-tested**
for the platforms marked ✅ below. **Telegram** runs today via the `regent-gateway` binary
(long-poll). The shared daemon **`POST /webhook/{platform}` HTTP route** is **live** — it builds
the adapter registry from whatever secrets are present in `.env`, verifies each inbound request,
runs the turn on a per-conversation session, and delivers the reply. **Discord** additionally has a
dedicated `POST /discord/interactions` route (Ed25519) enabled by `DISCORD_PUBLIC_KEY`.

### Support matrix

| Platform | Status | Inbound verification | Secrets needed |
|---|---|---|---|
| Telegram | ✅ live (poll) | bot token (webhook: secret-token header) | `REGENT_TELEGRAM_TOKEN` (+ `REGENT_TELEGRAM_ALLOWED_USERS`) |
| Slack | ✅ adapter | `v0=` HMAC-SHA256 of `v0:{ts}:{body}` + replay window | signing secret, bot token |
| Messenger | ✅ adapter | `X-Hub-Signature-256` HMAC-SHA256 | app secret, page token |
| WhatsApp | ✅ adapter | `X-Hub-Signature-256` HMAC-SHA256 | app secret, access token, phone-number id |
| LINE | ✅ adapter | `X-Line-Signature` base64 HMAC-SHA256 | channel secret, access token |
| Mattermost | ✅ adapter | shared token in body (constant-time) | base URL, verify token, bot token |
| Discord | ✅ adapter | Ed25519 over `{ts}{body}` (interactions route) **and** Gateway WebSocket | `DISCORD_PUBLIC_KEY` (interactions); bot token (Gateway) |
| Microsoft Teams | ✅ adapter (outgoing webhook) | `Authorization: HMAC <b64>` HMAC-SHA256 over body; **sync reply** | `TEAMS_OUTGOING_SECRET` |
| Google Chat | ✅ adapter | Google-signed RS256 bearer JWT (iss `chat@system…`, aud = project #) vs rotating JWKS; **sync reply** | `GCHAT_AUDIENCE` |
| Feishu / Lark | ✅ adapter | `X-Lark-Signature` SHA256 of `ts+nonce+key+body` + AES-256-CBC (or plaintext token) | `FEISHU_VERIFICATION_TOKEN` (+ `FEISHU_ENCRYPT_KEY`, `FEISHU_TENANT_TOKEN`) |
| WeCom (企业微信) | ✅ adapter | encrypted GET `echostr` + `msg_signature` SHA1 over query; **XML** + WXBizMsgCrypt AES | `WECOM_TOKEN`, `WECOM_ENCODING_AES_KEY`, `WECOM_AGENT_ID` (+ `WECOM_ACCESS_TOKEN`) |
| WeChat (公众号) | ✅ adapter | GET `echostr` + SHA1 over query params; **XML** body, optional WXBizMsgCrypt AES | `WECHAT_TOKEN` (+ `WECHAT_ENCODING_AES_KEY`, `WECHAT_ACCESS_TOKEN`) |
| SMS (Twilio) | ✅ adapter | `X-Twilio-Signature` HMAC-SHA1 over **URL + form params** | `TWILIO_ACCOUNT_SID`, `TWILIO_AUTH_TOKEN`, `TWILIO_FROM_NUMBER` |
| Voice Call (Twilio) | ✅ adapter | `X-Twilio-Signature` (URL+params); **TwiML** sync reply, built-in speech `<Gather>` | `TWILIO_AUTH_TOKEN`, `TWILIO_VOICE_GREETING` |
| Email (Mailgun) | ✅ adapter | Inbound-Parse HMAC-SHA256 (signature in body) | `MAILGUN_SIGNING_KEY`, `MAILGUN_API_KEY`, `MAILGUN_DOMAIN`, `MAILGUN_FROM` |
| Jira Cloud | ✅ adapter (events) | optional `X-Hub-Signature` HMAC-SHA256; issue/comment → summary; replies as ADF comment | `JIRA_EMAIL`, `JIRA_API_TOKEN`, `JIRA_BASE_URL` (+ `JIRA_WEBHOOK_SECRET`) |
| Azure DevOps | ✅ adapter (events) | Service-Hook Basic-auth check; `workitem.*`/`build.*` → summary; replies as work-item comment | `AZURE_DEVOPS_PAT`, `AZURE_DEVOPS_ORG_URL` (+ `_BASIC_USER`/`_BASIC_PASS`) |
| Trello | ✅ adapter | `X-Trello-Webhook` base64 HMAC-SHA1 over **body + callback URL**; HEAD/GET liveness 200 | `TRELLO_API_SECRET`, `TRELLO_API_KEY`, `TRELLO_TOKEN` |
| iMessage | ❌ no API | — | Apple ships no bot API; needs a self-hosted macOS bridge (e.g. BlueBubbles) |

**The shared contract** carries everything the adapters need: a full request context
(`verify_request(&WebhookRequest)` — body, signature, timestamp, nonce, **and** URL), a generalized
reply transport (`SendAuth::{None,Bearer,Basic}` × `SendBody::{Json,Form}`), synchronous-reply
support (`sync_reply`/`sync_response` returning JSON **or** TwiML/XML), a `GET` `echostr` handshake
(`verify_get`), and a `handshake(body)` for `url_verification`. That covers HMAC (Slack/Messenger/
WhatsApp/LINE/Mailgun), URL-signing (Twilio), Ed25519 (Discord), AES + XML (Feishu/WeChat/WeCom),
and RS256/JWKS (Google Chat). **iMessage is the only ❌** — Apple ships no server bot API, so it
would require a self-hosted macOS bridge (e.g. BlueBubbles); it's documented, not stubbed.

### Configuring a ✅ platform (example: Slack)

1. Create a Slack app, enable Event Subscriptions, subscribe to `message.channels`.
2. Add the **signing secret** and a **bot token** to `$REGENT_HOME/.env`
   (`SLACK_SIGNING_SECRET`, `SLACK_BOT_TOKEN`).
3. Point the Slack request URL at the daemon's `/webhook/slack`.

Messenger/WhatsApp/LINE/Mattermost/Twilio-SMS follow the same pattern with their own secrets
(matrix above). Behind a proxy, forward `X-Forwarded-Proto`/`X-Forwarded-Host` so the daemon can
reconstruct the public URL that Twilio signs.

### iMessage — unsupported (by design)

Apple ships **no server-side bot or webhook API** for iMessage: Messages for Business is invite-only
and contract-gated, and there is no public inbound/outbound message API. So Regent has **no
`imessage` adapter** — there's nothing to verify or call, and shipping a stub would be dishonest.

If you must bridge iMessage, the only route is a **self-hosted macOS bridge** that drives the
Messages app on a always-on Mac (e.g. [BlueBubbles](https://bluebubbles.app) or an AppleScript/SQLite
poller) and re-exposes it as an HTTP webhook. Such a bridge produces ordinary signed POSTs, at which
point a thin `WebhookAdapter` (HMAC over the body + a `send_request` to the bridge) drops into the
same contract as every other platform — no core changes needed. That bridge is out of scope here;
it's an operational dependency, not a Regent feature.

## Sandboxing tool execution

Tool execution is guarded in layers — important once external chat platforms can trigger turns:

1. **Approval gate** (always on): dangerous commands (`rm -rf`, `mkfs`, `curl … | sh`, force-push, …)
   route through a human approval prompt, deny-by-default.
2. **Filesystem jail**: every file tool (`read_file`/`write_file`/`search_files`)
   and the `terminal` cwd is contained to the session workspace — `..` traversal, symlink escapes,
   and absolute paths outside the workspace are refused. Your secrets in `$REGENT_HOME` (`.env`,
   `config.yaml`) sit outside the workspace, so a sandboxed turn can't read or rewrite them.
   **Externally-triggered sessions (platform webhooks, gateway conversations) are ALWAYS jailed** —
   `REGENT_SANDBOX=1` extends the same jail to local CLI sessions. External sessions' memory
   writes are also staged for your approval (`regent memory pending` → `approve`/`reject`)
   instead of committing directly (ADR-030).
3. **Isolated command execution**: choose a backend via `REGENT_TERMINAL_BACKEND`:
   - `local` (default) — host shell, no isolation.
   - `docker:<container>[:workdir]` — `docker exec` into a standing container.
   - `sandbox:<image>` — a fresh, locked-down `docker run` per command (`--network none`,
     `--read-only`, `--cap-drop ALL`, `no-new-privileges`, memory/pid caps; only `/work` + `/tmp`
     writable). **Recommended for untrusted input.**
   - `ssh:<user@host>` — run on a remote box (key-based, `BatchMode`).

**Enforce it:** with `REGENT_SANDBOX=1`, the host `local` backend is **refused** — the daemon fails
to start with a clear error unless `REGENT_TERMINAL_BACKEND` is `sandbox:`/`docker:`/`ssh:`. It never
silently degrades to unsandboxed execution.

```bash
# Strongest posture for an externally-reachable daemon:
export REGENT_SANDBOX=1
export REGENT_TERMINAL_BACKEND=sandbox:alpine
```

## Voice calls (with vision)

```bash
regent call        # local call UI: talk, it talks back
```

Speech runs locally (whisper ASR + Kokoro TTS via sherpa-onnx; ~900MB of models
auto-download on first run). Building the voice server needs LLVM/libclang — see
[development/voice-and-api-calls.md](development/voice-and-api-calls.md). On a call the agent
can **see**: your screen ("are you seeing what I'm seeing?", via computer-use screenshots)
and your camera ("what am I holding?" — allow camera when the call UI asks; deny it and the
call is audio-only). Mutating actions (clicks, keys, app/browser control, file
edits, terminal) are denied by default because a call has no approval dialog.
Set `REGENT_VOICE_FULL_CONTROL=1` only when you intentionally want hands-free
mutation; read-only screen/camera vision remains available without it.

Ask for something big on a call — "build me an expense tracker", "research X and write it
up" — and it runs **in the background**: Regent says it's started, the call keeps flowing
(ask "how's it going?" anytime), and it reports the result the next time you speak after
the job finishes. Long thinks are fine too; the call no longer resets on them.

## Documents

Ask for a deck, report, spreadsheet, or PDF and the bundled `documents` skill uses Regent's
native `read_document` and `create_document` tools to produce a real PPTX, DOCX, XLSX, or PDF
and return its path. No Python, Node, package installation, or extra setup is required.

## Profiles

`regent -p work chat` isolates all state under `~/.regent-profiles/work` (its own `.env`,
`config.yaml`, db). Handy for separating personal/work bots and credentials.

## Going deeper

- Every command: [reference/commands.md](reference/commands.md)
- Every environment variable: [reference/env-vars.md](reference/env-vars.md)
- Architecture & repo navigation: [../README.md](../README.md) + [adr/](adr/)
