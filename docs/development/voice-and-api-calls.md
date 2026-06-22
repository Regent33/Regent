# Voice & API calls

How Regent reaches external models — the **chat LLM**, **ASR/TTS** (speech), and
**vision** — and where each is configured.

## Config sources
- **`$REGENT_HOME/config.yaml`** (default `~/.regent/config.yaml`) — non-secret
  settings (provider, model, speech). The **daemon** reads this; it reloads each run.
- **`$REGENT_HOME/.env`** — secrets only (API keys), `KEY=value` per line, `0600`.
- **Environment variables** — the **gateway** binary is env-driven (it does not
  read `config.yaml`); some daemon overrides also come from env.

Manage them with: `regent setup` (model + key), `regent keys` (keys),
`regent voice setup` (speech), `regent config` (view).

---

## 1. The chat model (the LLM)
Configured under `model:` in `config.yaml`:
```yaml
model:
  provider: anthropic        # anthropic | openai | openrouter | groq | deepseek | together | ollama
  default: claude-sonnet-4-6
  base_url:                  # optional override; empty = the provider's own endpoint
```
`anthropic` uses the native Messages API (prompt-cache breakpoints); every other
value is an **OpenAI-compatible** endpoint differing only by base URL. The key
lives in `.env` (`REGENT_API_KEY`, or a provider-specific var). `regent setup`
writes both.

**Gateway** uses env instead: `REGENT_API_KEY`, `REGENT_MODEL`,
`REGENT_BASE_URL` (default OpenRouter).

---

## 2. Speech: ASR (speech→text) + TTS (text→speech)
**Off by default** — a fresh install downloads no model and makes no speech calls.

### Daemon (config.yaml)
```yaml
speech:
  enabled: false                 # turn on with `regent voice setup` / `regent voice enable`
  models_dir: ~/.regent/models
  asr: { provider: local, model: qwen3-asr-1.7b, language: auto, base_url: "", weights: [] }
  tts: { provider: local, model: qwen3-tts-1.7b, voice: default, format: opus, base_url: "", weights: [] }
  vision: { input_mode: auto, provider: auto, model: "", download_timeout: 30 }
  call: { fast_model: "" }       # a fast model for quick spoken turns; empty = use the main model
```

Commands:
```bash
regent voice setup       # pick provider/model, download weights, enable (download-on-enable)
regent voice status      # enabled? which providers/models? available?
regent voice models      # configured + built-in provider names
regent voice enable | disable
```

**Providers** (a registry name → resolved base URL + key):
| provider | base URL | key (.env) |
|---|---|---|
| `local` | `http://localhost:8000/v1` (a Qwen3 server you run, e.g. vLLM/Ollama) | none |
| `groq` | `https://api.groq.com/openai/v1` | `GROQ_API_KEY` |
| `openai` | `https://api.openai.com/v1` | `OPENAI_API_KEY` |
| `qwen` / `dashscope` | DashScope compatible-mode | `DASHSCOPE_API_KEY` |

**Download-on-enable:** `voice setup`/`enable` call the daemon's
`voice.ensure_models`, which downloads each model's `weights` (`{name,url,sha256}`)
into `models_dir` (checksum-verified, idempotent). Empty `weights` ⇒ nothing to
download (a hosted provider, or a localhost server you run yourself). Weight URLs
must be **HTTPS** (loopback exempt) and are size-capped.

### Quick start (recommended)
```
regent voice setup     # pick a provider from the menu, paste a key — done
regent voice test      # verify TTS works end to end
```
`regent voice setup` is a guided menu (easiest first: **Groq** for free Whisper,
**OpenAI** for STT+spoken replies, **Qwen/DashScope**, or a **local** Qwen3
server). One run writes **both** planes: `config.yaml` (the daemon, for chat) and
`.env` (`REGENT_SPEECH_*`, for the gateway/Telegram) — so voice works everywhere
from one command.

### Gateway (env — for Telegram voice)
The gateway reads speech from env (it doesn't read `config.yaml`). `regent voice
setup` writes these for you; you can also set them by hand:
```
REGENT_SPEECH_BASE_URL=https://api.groq.com/openai/v1   # or http://localhost:8000/v1
REGENT_SPEECH_API_KEY=<key>                              # empty for a localhost server
REGENT_SPEECH_ASR_MODEL=qwen3-asr-1.7b                   # optional
REGENT_SPEECH_TTS_MODEL=qwen3-tts-1.7b                   # optional
```
Voice is **opt-in**: the gateway enables it only when `REGENT_SPEECH_BASE_URL` is
set. The CLI writes to `$REGENT_HOME/.env`; ensure the gateway process loads it
(or set the vars in its environment).

---

## 3. How a speech call is actually made
One **OpenAI-compatible HTTP adapter** serves every provider, differing only by
base URL + key (in `regent-speech`):

- **ASR:** `POST {base}/audio/transcriptions` — multipart `file` + `model`
  (`response_format=text`). A Telegram voice note's OGG/Opus bytes are sent
  **as-is** (`voice.ogg`) — Whisper-style endpoints accept ogg/mp3/m4a/wav, so no
  local decode is needed.
- **TTS:** `POST {base}/audio/speech` — JSON `model`/`voice`/`input`/
  `response_format` (Opus for chat voice bubbles).

The HTTP call is **injected** (`HttpExecutor`, a reqwest impl) so `regent-speech`
stays network-free and unit-testable. Sync trait + blocking executor, always run
off the runtime via `spawn_blocking`.

### Telegram voice round-trip (turn-based)
```
voice note ─► getFile (≤20 MB) ─► download OGG ─► ASR transcribe_file ─► text turn
                                                                            │
                                                                       agent runs
                                                                            │
text reply ◄─ sendVoice (Opus) ◄─ TTS synthesize ◄────────────────────────┘
   (only when the chat last spoke; falls back to text on any failure)
```
Enable it by setting `REGENT_SPEECH_BASE_URL` (+ key) before starting the gateway.
A provider must do **both** ASR and Opus TTS for a full spoken round-trip; ASR-only
(e.g. Groq) gives voice-in / text-out.

---

## 4. Vision
`speech.vision.input_mode`: `auto | native | text`.
- **text** — the agent runs a `vision_analyze` tool and works from a description
  (any text model).
- **native** — image parts ride on the model turn (vision-capable models).
- **auto** — native if the active model reports vision capability, else text.

Remote image fetches are size-capped (decompression-bomb guard) and run an SSRF
check. (Vision wiring is in progress; see the realtime-AV plan in `docs/`.)

---

## 5. Other API calls
- **MCP tools** — point `REGENT_BROWSER_MCP_URL` (or other MCP servers) and the
  agent gains their tools; mutating actions are approval-gated.
- **Web search/fetch** — pluggable across providers; keys via `regent keys`.
- All outbound HTTP is HTTPS; keys come from `.env`/env and are never logged.
