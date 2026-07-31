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
| `REGENT_PROVIDER` | Provider kind — see [the full list](#provider-kinds) | config `model.provider` |
| `REGENT_BASE_URL` | Override the provider endpoint | provider's own default |
| `REGENT_DEACON_PATH` | Explicit path to the regent-deacon binary | auto-discovery |
| `REGENT_LOG` | Log filter (tracing syntax) | `info` |
| `REGENT_NOW` | Frozen clock for tests | real time |
| `REGENT_BANNER` | CLI banner toggle | on |
| `REGENT_KEEPALIVE` | Deacon serves cron/board loops after stdin closes | off |
| `REGENT_REPO_DIR` | Repo root override (dev tooling) | auto |
| `REGENT_NO_UPDATE_CHECK` | `1` disables the background release check. Notify-only either way — Regent never downloads or installs an update; it only tells you when a newer release exists and links the official download page. | off (check once a day) |

## Provider kinds

The value `REGENT_PROVIDER` (and config `providers.<name>.kind`) accepts. Each
one is an OpenAI-compatible endpoint except `anthropic`, which uses the native
Messages API. Every kind reads its own key var, falling back to
`REGENT_API_KEY`; set the key in Settings → API Keys, `regent keys`, or
`$REGENT_HOME/.env`. `base_url` overrides the endpoint for any of them.

| Kind | Key var |
|---|---|
| `anthropic` | `ANTHROPIC_API_KEY` |
| `openai` · `openrouter` | `OPENAI_API_KEY` · `OPENROUTER_API_KEY` |
| `groq` · `deepseek` · `together` | `GROQ_API_KEY` · `DEEPSEEK_API_KEY` · `TOGETHER_API_KEY` |
| `mistral` · `xai` · `gemini` | `MISTRAL_API_KEY` · `XAI_API_KEY` · `GEMINI_API_KEY` |
| `moonshot` · `zhipu` · `dashscope` | `MOONSHOT_API_KEY` · `ZHIPU_API_KEY` · `DASHSCOPE_API_KEY` |
| `fireworks` · `cerebras` · `perplexity` | `FIREWORKS_API_KEY` · `CEREBRAS_API_KEY` · `PERPLEXITY_API_KEY` |
| `minimax` · `nvidia` | `MINIMAX_API_KEY` · `NVIDIA_API_KEY` |
| `sambanova` · `hyperbolic` · `novita` | `SAMBANOVA_API_KEY` · `HYPERBOLIC_API_KEY` · `NOVITA_API_KEY` |
| `deepinfra` · `siliconflow` · `nebius` | `DEEPINFRA_API_KEY` · `SILICONFLOW_API_KEY` · `NEBIUS_API_KEY` |
| `chutes` · `venice` · `cohere` | `CHUTES_API_KEY` · `VENICE_API_KEY` · `COHERE_API_KEY` |
| `github-models` | `GITHUB_TOKEN` (a PAT) |
| `ollama-cloud` · `ollama` (local) | `OLLAMA_API_KEY` · none |
| `lmstudio` · `llamacpp` · `vllm` · `litellm` (all local) | none by default |

The four local server kinds default to their documented ports —
`localhost:1234`, `:8080`, `:8000`, `:4000` — and need no key unless you have
put your own proxy behind one.

## Security & sandboxing

| Variable | Meaning | Default |
|---|---|---|
| `REGENT_SANDBOX` | `1` = jail file tools + forbid the host `local` terminal backend for **local** sessions. External (webhook/platform) sessions are **always** jailed regardless (ADR-030). | off |
| `REGENT_TERMINAL_BACKEND` | `local` · `docker:<container>` · `sandbox:<image>` · `ssh:<user@host>` | `local` |
| `REGENT_AUTO_APPROVE` | `1` selects an env-fixed approval posture. Voice sets it but still denies mutations unless full control is explicit | off |
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
| `REGENT_MODELS_DIR` / `REGENT_WHISPER_DIR` / `REGENT_WHISPER_SIZE` / `REGENT_WHISPER_LANG` | Local ASR model location/size/language (`LANG` empty/unset = automatic recognition) | `$REGENT_HOME/models`… / auto language |
| `REGENT_KOKORO_DIR` / `REGENT_KOKORO_SPEAKER` / `REGENT_TTS_ENGINE` | Local TTS voice configuration | defaults |
| `REGENT_CALL_UI_ORIGIN` | Extra allowed CORS origin for the call UI | localhost:3000 only |
| `REGENT_BRAIN_MODEL` | Model override for the call agent | `REGENT_MODEL` |
| `REGENT_SPEECH_PROVIDER` / `REGENT_SPEECH_API_KEY` / `REGENT_SPEECH_BASE_URL` / `REGENT_SPEECH_ASR_MODEL` / `REGENT_SPEECH_TTS_MODEL` | Hosted speech (instead of local ONNX) | local |

### Speech providers

`speech.asr.provider` / `speech.tts.provider` take any id below; run
`regent voice models` to see the same list with what each one is configured
for. Hosted ones need their key var, self-hosted ones need nothing. Setting
`speech.*.base_url` overrides the endpoint for any provider — and is how an id
that is not listed here reaches the same adapter.

| Provider | Does | Key var |
|---|---|---|
| `groq` · `openai` · `qwen` (`dashscope`) | both | `GROQ_API_KEY` · `OPENAI_API_KEY` · `DASHSCOPE_API_KEY` |
| `deepinfra` · `lemonfox` · `siliconflow` | both | `DEEPINFRA_API_KEY` · `LEMONFOX_API_KEY` · `SILICONFLOW_API_KEY` |
| `aimlapi` | both | `AIMLAPI_API_KEY` |
| `fireworks` · `together` · `mistral` · `novita` · `sambanova` | speech-to-text only | their `*_API_KEY` |
| `azure` · `runpod` · `custom` | both — **`base_url` required** | `AZURE_OPENAI_API_KEY` · `RUNPOD_API_KEY` · `REGENT_SPEECH_API_KEY` |
| `local` (vLLM) · `speaches` · `localai` · `litellm` · `voxbox` | both | none |
| `whispercpp` · `koboldcpp` | speech-to-text only | none |
| `kokoro` · `openedai` · `edge` · `orpheus` · `chatterbox` · `alltalk` | text-to-speech only | none |

Providers that do not speak the OpenAI audio wire (ElevenLabs, Deepgram) are
deliberately absent rather than listed and broken — they need custom headers and
a raw-bytes body the adapter cannot build yet.

The bundled Butler call server is separate from all of this: it always runs
local Whisper + Kokoro and is configured by the `REGENT_WHISPER_*` /
`REGENT_KOKORO_*` vars above.

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

## Document rendering (`create_document`)

The renderer is a hidden `regent __render` subcommand; PDF/PPTX prefer it and fall
back to the native writers when it or a browser is absent (see ADR-040).

| Variable | Meaning | Default |
|---|---|---|
| `REGENT_CLI_PATH` | Explicit path to the compiled `regent-cli` used as the render sidecar | dev source → `dist/regent-cli` → `regent` on PATH |
| `REGENT_CHROMIUM_PATH` | Browser executable for HTML→PDF and headless preview screenshots | installed Chrome/Edge/Chromium |
| `REGENT_SOFFICE_PATH` | LibreOffice `soffice` binary for headless deck (`.pptx`) previews | standard install paths / PATH; without it deck preview is skipped with a note |

## Platforms (gateway long-poll)

| Variable | Meaning |
|---|---|
| `REGENT_TELEGRAM_TOKEN` | Telegram bot token (the gateway binary + webhook plane) |
| `REGENT_TELEGRAM_ALLOWED_USERS` | Comma-separated allowed user ids |
| `REGENT_TELEGRAM_ALLOW_ALL` | Disable the allowlist (not recommended) |

## Installer

Read by the one-line install/uninstall scripts (`scripts/install.*`,
`scripts/uninstall.*`), not the running binary. The GUI installer sets
`REGENT_LOCAL_ARCHIVE`, `REGENT_NO_PATH`, and `REGENT_NO_LAUNCH` for you.

| Variable | Meaning | Default |
|---|---|---|
| `REGENT_REPO` | GitHub `owner/repo` used for release downloads and source fallback | `Regent33/Regent` |
| `REGENT_BIN_DIR` | Binary install/removal directory | `$REGENT_HOME/bin` for one-line installs |
| `REGENT_LINK_DIR` | macOS/Linux directory that receives the `regent` symlink | `~/.local/bin` |
| `REGENT_SRC_DIR` | Source checkout used only when no verified release is available | `$REGENT_HOME/src` |
| `REGENT_LOCAL_ARCHIVE` | Path to a bundled release archive; installs from it offline, skipping the download and network checksum | off (download the latest GitHub release) |
| `REGENT_NO_PATH` | Skip putting `regent` on PATH (the symlink on macOS/Linux, the user PATH entry on Windows) | off (add to PATH) |
| `REGENT_NO_LAUNCH` | Skip the first-time `setup` wizard the installer runs after a successful install | off (setup runs when a terminal is attached) |
| `REGENT_NO_FFMPEG` | Skip provisioning ffmpeg for camera capture (Windows fetches a pinned build; macOS/Linux only print a package-manager hint) | off (provision if ffmpeg is absent) |
| `REGENT_PURGE` | Uninstaller only: `1` also deletes your data under `$REGENT_HOME` (config, keys, sessions, memory, source checkout) | off (data kept) |

> `REGENT_TEST_*` variables are test fixtures only — never set them.
