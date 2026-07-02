# Reference — Environment variables

Every variable Regent actually reads, reconciled against the code (grep
`REGENT_` under `src/` to re-verify). Set them in your shell or in
`$REGENT_HOME/.env` (shell wins). Platform webhook secrets (SLACK_*, WHATSAPP_*,
TWILIO_*, …) are listed in [QUICKSTART §6](../QUICKSTART.md#6-messaging-platforms)
and manageable via `regent keys`.

## Core

| Variable | Meaning | Default |
|---|---|---|
| `REGENT_HOME` | State directory (.env, config.yaml, db, skills, voice models) | `~/.regent` |
| `REGENT_API_KEY` | Model provider API key | — (ollama needs none) |
| `REGENT_MODEL` | Model id for new sessions | config `model.default` |
| `REGENT_PROVIDER` | Provider kind (anthropic/openai/openrouter/groq/deepseek/together/ollama) | config `model.provider` |
| `REGENT_BASE_URL` | Override the provider endpoint | provider's own default |
| `REGENT_DEACON_PATH` | Explicit path to the regent-deacon binary | auto-discovery |
| `REGENT_LOG` | Log filter (tracing syntax) | `info` |
| `REGENT_NOW` | Frozen clock for tests | real time |
| `REGENT_BANNER` | CLI banner toggle | on |
| `REGENT_KEEPALIVE` | Deacon serves cron/board loops after stdin closes | off |
| `REGENT_REPO_DIR` | Repo root override (dev tooling) | auto |

## Security & sandboxing

| Variable | Meaning | Default |
|---|---|---|
| `REGENT_SANDBOX` | `1` = jail file tools + forbid the host `local` terminal backend for **local** sessions. External (webhook/platform) sessions are **always** jailed regardless (ADR-030). | off |
| `REGENT_TERMINAL_BACKEND` | `local` · `docker:<container>` · `sandbox:<image>` · `ssh:<user@host>` | `local` |
| `REGENT_AUTO_APPROVE` | `1` = auto-approve gated actions (voice sets this; scoped — see below) | off |
| `REGENT_HTTP_ENABLED` / `REGENT_HTTP_BIND` / `REGENT_HTTP_TOKEN` | REST ingress `/v1/chat`; refuses to start without a token | off |

## Voice & calls

| Variable | Meaning | Default |
|---|---|---|
| `REGENT_VOICE` | Marks a voice deacon (spoken replies; scoped approver) | off |
| `REGENT_VOICE_AUTO_APPROVE` | Voice sets `REGENT_AUTO_APPROVE=1` unless this is `0` | on |
| `REGENT_VOICE_FULL_CONTROL` | `1` = voice auto-approve is blanket again (desktop/terminal mutations allowed). Default: mutations **denied**, vision/screen reads unaffected | off |
| `REGENT_VOICE_COMPUTER_USE` | Give the voice deacon computer-use (screen) | on |
| `REGENT_VOICE_PORT` / `REGENT_VOICE_SERVER_PATH` / `REGENT_VOICE_AGENT` | Voice server port / binary path / agent toggle | 8130 / auto / on |
| `REGENT_VOICE_AUTODOWNLOAD` | Fetch ASR/TTS model files on first run | on |
| `REGENT_MODELS_DIR` / `REGENT_WHISPER_DIR` / `REGENT_WHISPER_SIZE` / `REGENT_WHISPER_LANG` | Local ASR model location/size/language | `$REGENT_HOME/models`… |
| `REGENT_KOKORO_DIR` / `REGENT_KOKORO_SPEAKER` / `REGENT_TTS_ENGINE` | Local TTS voice configuration | defaults |
| `REGENT_CALL_UI_ORIGIN` | Extra allowed CORS origin for the call UI | localhost:3000 only |
| `REGENT_BRAIN_MODEL` | Model override for the call agent | `REGENT_MODEL` |
| `REGENT_SPEECH_PROVIDER` / `REGENT_SPEECH_API_KEY` / `REGENT_SPEECH_BASE_URL` / `REGENT_SPEECH_ASR_MODEL` / `REGENT_SPEECH_TTS_MODEL` | Hosted speech (instead of local ONNX) | local |

## Tools

| Variable | Meaning | Default |
|---|---|---|
| `REGENT_COMPUTER_USE` | Enable the desktop-control toolset | off |
| `REGENT_COMPUTER_USE_BACKEND` / `REGENT_CUA_DRIVER_CMD` | Computer-use driver selection | built-in |
| `REGENT_SEARCH_PROVIDER` / `REGENT_SEARCH_API_KEY` | Web search (brave/tavily/serpapi/exa/google_cse/duckduckgo) | duckduckgo, keyless |
| `REGENT_VISION_MODEL` / `REGENT_VISION_API_KEY` / `REGENT_VISION_BASE_URL` | Vision analysis model | gemini-flash via OpenRouter, falls back to `REGENT_API_KEY` |
| `REGENT_IMAGE_MODEL` / `REGENT_IMAGE_API_KEY` / `REGENT_IMAGE_BASE_URL` | Image generation | defaults |
| `REGENT_VIDEO_MODEL` | Video analysis model | default |
| `REGENT_BROWSER_MCP_URL` | Attach a Playwright MCP server for browser control | off |
| `REGENT_REVEAL_FILES` | Reveal tool file allowlist | — |

## Platforms (gateway long-poll)

| Variable | Meaning |
|---|---|
| `REGENT_TELEGRAM_TOKEN` | Telegram bot token (the gateway binary + webhook plane) |
| `REGENT_TELEGRAM_ALLOWED_USERS` | Comma-separated allowed user ids |
| `REGENT_TELEGRAM_ALLOW_ALL` | Disable the allowlist (not recommended) |

> `REGENT_TEST_*` variables are test fixtures only — never set them.
