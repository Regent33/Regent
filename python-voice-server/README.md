# python-voice-server

Local, no-API-key speech for Regent: **Qwen3-ASR-1.7B** (speech→text) and
**Qwen3-TTS-1.7B-CustomVoice** (text→speech) behind an OpenAI-compatible HTTP API,
plus a hands-free browser voice call at `/call`.

```
mic → Qwen3 ASR → Regent's model → Qwen3 TTS → speaker      (turn by turn)
```

## Run

```bash
regent voice serve          # finds Python, checks deps, launches this server
# or directly:
python python_server.py     # → http://localhost:8000  (/call for the voice call)
```

Models are expected under `tts-asr-local-models/` (override with `REGENT_MODELS_DIR`).
The server **warms both models in the background at startup**, so the first call
skips the 10–30 s cold-load cliff.

## Latency — read this first

Per-turn time is **dominated by the two 1.7B models**, not the server code. In
order of impact:

### 1. GPU — the real fix (5–30× over CPU)

The server auto-detects CUDA (`device=cuda:0`). If it prints `⚠ running on CPU`,
install a CUDA torch build for your RTX card — **one command**:

```bash
pip install --force-reinstall torch torchaudio --index-url https://download.pytorch.org/whl/cu124
```

(Use the index matching your driver: `cu121`, `cu124`, … — check `nvidia-smi`.)
Restart the server; it should now print `GPU: <your card>` and `device=cuda:0`.
Force a device with `REGENT_SPEECH_DEVICE=cuda:0` (or `cpu`).

### 2. Staying on CPU

- Warming (automatic) removes the first-call cliff.
- Keep replies short — the call's system prompt already asks for 1–2 sentences.
- `torch.set_num_threads` is maxed to all cores automatically.
- Language- and model-level quantization (int8) is the next CPU lever; not wired
  yet. A Rust/ONNX rewrite does **not** help (the bottleneck is model compute, and
  these custom 1.7B speech models aren't ONNX-exportable today — see
  `docs/voice-onnx-feasibility.md`).

## Env vars

| Var | Default | Meaning |
|---|---|---|
| `REGENT_MODELS_DIR` | `tts-asr-local-models` | weights directory |
| `REGENT_SPEECH_DEVICE` | auto (`cuda:0`/`cpu`) | force a torch device |
| `REGENT_SPEECH_LANG` | `English` | TTS language |
| `REGENT_SPEECH_SPEAKER` | `Ryan` | CustomVoice speaker |
| `REGENT_SPEECH_INSTRUCT` | conversational | TTS delivery style |
| `REGENT_BRAIN_*` / `REGENT_*` | — | reply model (base url / key / model) |

## Endpoints

- `POST /v1/audio/transcriptions` — OpenAI-compatible ASR
- `POST /v1/audio/speech` — OpenAI-compatible TTS
- `GET /`, `GET /call` — status page + the hands-free voice call (`ui/`)
- `GET /health` — `{asr, tts, device, models_dir}`
