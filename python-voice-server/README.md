# python-voice-server

Local, no-API-key speech for Regent: **faster-whisper** (speech→text) and
**Piper** (text→speech) behind an OpenAI-compatible HTTP API, plus a hands-free
browser voice call at `/call`.

```
mic → faster-whisper ASR → Regent's model → Piper TTS → speaker   (turn by turn)
```

This is also the **speech backend for the native `regent call`** (the LiveKit /
Next.js UI): its local provider POSTs to this server's `/v1/audio/*` endpoints.

## Run

```bash
regent voice serve          # finds Python, checks deps, launches this server
# or directly:
python python_server.py     # → http://localhost:8000  (/call for the voice call)
```

The server **warms both models at startup**, so the first call skips the cold load.

## Install

```bash
pip install faster-whisper piper-tts soundfile
```

For the **GPU ASR path** (recommended — sub-second transcription), also install the
CUDA build of torch, which provides the CUDA runtime faster-whisper/CTranslate2 uses:

```bash
pip install --force-reinstall torch --index-url https://download.pytorch.org/whl/cu128
```

(Use the index matching your driver — `cu126`, `cu128`, … — check `nvidia-smi`.) The
server auto-detects CUDA; force it with `REGENT_SPEECH_DEVICE=cuda` or `cpu`.

## Latency

Real-time on a laptop. Measured on an RTX 4060 Laptop (8 GB):

| Stage | Engine | Time |
|---|---|---|
| ASR | faster-whisper `small`, GPU int8 | **~0.2–0.6 s** |
| TTS | Piper, CPU | **~0.1 s** (≈33× faster than realtime) |

Per turn ≈ **ASR + brain LLM + TTS ≈ 1–2 s**.

> **Why not Qwen3-1.7B?** The previous stack (Qwen3-ASR-1.7B + Qwen3-TTS-1.7B) was
> **~70 s/turn** here: both bf16 models are ~8.3 GB and don't fit in 8 GB VRAM
> together (CUDA pages to system RAM → thrash), and even TTS-alone-on-GPU was ~10 s.
> faster-whisper + Piper are an order of magnitude lighter for the same job. The
> Qwen weights under `tts-asr-local-models/` are no longer used by this server.

## Env vars

| Var | Default | Meaning |
|---|---|---|
| `REGENT_SPEECH_DEVICE` | auto (`cuda`/`cpu`) | ASR device |
| `REGENT_WHISPER_SIZE` | `small` | `tiny`·`base`·`small`·`medium`·`large-v3` |
| `REGENT_PIPER_VOICE` | `en_US-lessac-medium` | downloaded on first run to `<models>/piper-voices/` |
| `REGENT_MODELS_DIR` | `tts-asr-local-models` | where the Piper voice is stored |
| `REGENT_MODEL` / `REGENT_BASE_URL` / `REGENT_API_KEY` | — | the call's brain (set by `regent voice serve`) |

## Endpoints

- `POST /v1/audio/transcriptions` — OpenAI-compatible ASR
- `POST /v1/audio/speech` — OpenAI-compatible TTS
- `GET /`, `GET /call` — status page + the hands-free voice call (`ui/`)
- `GET /health` — `{engine, asr, tts, device, models_dir}`
