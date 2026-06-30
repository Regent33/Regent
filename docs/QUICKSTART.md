# Regent — Quickstart

Get Regent running, connect a model, and (optionally) wire a chat platform.

## 1. Build

```bash
# Rust core (Orchustr must be a sibling checkout — see Cargo.toml path deps)
cargo build --release -p regent-deacon
# CLI (TypeScript/Ink, compiled to a single self-contained binary with Bun)
cd src/regent-cli && bun install && bun run compile
```

This produces `target/release/regent-deacon` (+ `regent-mcp`) and `src/regent-cli/dist/regent-cli`.
The CLI locates the daemon via `REGENT_DEACON_PATH`, a sibling binary, `PATH`, or the cargo
`target/` dir — so a dev build is found automatically. (During development you can skip the compile
and run `bun run dev` from `src/regent-cli`.)

### Install the `regent` command (so it works in any terminal)

The compiled binary is `src/regent-cli/dist/regent-cli(.exe)`. Put it on your PATH so `regent …`
runs as a shell command (otherwise you have no `regent` — only `bun run dev`, which is just the chat):

- **Windows:** create `%USERPROFILE%\.bun\bin\regent.cmd` (that dir is already on PATH) containing:
  ```bat
  @echo off
  "<repo>\src\regent-cli\dist\regent-cli.exe" %*
  ```
- **macOS/Linux:** `ln -s "$PWD/dist/regent-cli" ~/.local/bin/regent` (or copy it onto your PATH).

After CLI code changes, re-run `bun run compile` to refresh what `regent` runs. Verify with
`regent --version` and `regent doctor`.

## 2. First-time setup

```bash
regent setup            # interactive: provider, model, API key
# or non-interactive:
regent setup --provider anthropic --model claude-sonnet-4-6 --key sk-ant-...
```

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
regent doctor           # toolchain, db, provider reachability, config lint
regent chat             # interactive streaming chat (or just `regent`)
regent logs --follow    # tail the redacted rolling log
```

## 4. Providers

`provider:` in `config.yaml` (or `--provider`) selects the backend. Anthropic uses the native
Messages API; the rest are OpenAI-compatible and differ only by base URL:

| provider | default host | notes |
|---|---|---|
| `anthropic` | api.anthropic.com | native, prompt-cache breakpoints |
| `openai` | openrouter.ai | historical default |
| `openrouter` | openrouter.ai | hundreds of models, one key |
| `groq` | api.groq.com | fast hosted open models |
| `deepseek` | api.deepseek.com | |
| `together` | api.together.xyz | |
| `ollama` | localhost:11434 | local, no key |

Any other OpenAI-compatible host works via `provider: openai` + `base_url: <url>`.

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
| Telegram | ✅ live (poll) | bot token (webhook: secret-token header) | `TELEGRAM_BOT_TOKEN` |
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
2. **Filesystem jail** (`REGENT_SANDBOX=1`): every file tool (`read_file`/`write_file`/`search_files`)
   and the `terminal` cwd is contained to the session workspace — `..` traversal, symlink escapes,
   and absolute paths outside the workspace are refused. Your secrets in `$REGENT_HOME` (`.env`,
   `config.yaml`) sit outside the workspace, so a sandboxed turn can't read or rewrite them.
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

## Profiles

`regent -p work chat` isolates all state under `~/.regent-profiles/work` (its own `.env`,
`config.yaml`, db). Handy for separating personal/work bots and credentials.
