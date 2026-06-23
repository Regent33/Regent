#!/usr/bin/env python3
"""
Local OpenAI-compatible speech server for Regent's `local` voice provider.

Serves the two endpoints Regent's `OpenAiCompat` ASR/TTS clients call, against
the Qwen3 weights staged in ../tts-asr-local-models:

    POST /v1/audio/transcriptions   (multipart: file, model)        -> {"text": ...}
    POST /v1/audio/speech           (json: input, voice, response_format) -> audio bytes

Quick start (use your real ML Python — per the about-profile, Python 3.14):
    # qwen-asr/qwen-tts pin different transformers builds; install in two steps:
    pip install qwen-asr soundfile librosa torchaudio sox einops
    pip install --no-deps qwen-tts
    python scripts/local_speech_server.py            # serves on http://localhost:8000
    # then, in another terminal:
    regent voice setup --provider local              # base_url http://localhost:8000/v1
    regent voice test

A CUDA GPU is strongly recommended (1.7B models are slow on CPU). Set
REGENT_SPEECH_DEVICE=cpu to force CPU, REGENT_SPEECH_LANG=English for the TTS
language, REGENT_MODELS_DIR to override the weights location.

Inference uses the official packages (qwen_tts / qwen_asr); see the model cards:
  https://huggingface.co/Qwen/Qwen3-ASR-1.7B
  https://huggingface.co/Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice
"""
from __future__ import annotations

import io
import os
import sys
import tempfile
import threading
from pathlib import Path

# Windows pipes/redirects default to cp1252, which crashes on non-ASCII output
# (our "→", or any unicode a model/lib prints). Force UTF-8 so logging never dies.
for _stream in (sys.stdout, sys.stderr):
    try:
        _stream.reconfigure(encoding="utf-8", errors="replace")
    except (AttributeError, ValueError):
        pass

import numpy as np
import soundfile as sf
import torch
import uvicorn
from fastapi import FastAPI, Form, Request, UploadFile
from fastapi.responses import JSONResponse, Response

MODELS_DIR = Path(os.environ.get("REGENT_MODELS_DIR", "tts-asr-local-models")).resolve()
ASR_DIR = MODELS_DIR / "Qwen3-ASR-1.7B"
TTS_DIR = MODELS_DIR / "Qwen3-TTS-12Hz-1.7B-CustomVoice"
DEVICE = os.environ.get("REGENT_SPEECH_DEVICE") or ("cuda:0" if torch.cuda.is_available() else "cpu")
DTYPE = torch.bfloat16 if "cuda" in DEVICE else torch.float32
LANG = os.environ.get("REGENT_SPEECH_LANG", "English")
# CustomVoice requires a speaker; pick an English-native one. Others: Serena,
# Vivian, Uncle_Fu, Aiden, Ono_Anna, Sohee, Eric, Dylan (see the model README).
SPEAKER = os.environ.get("REGENT_SPEECH_SPEAKER", "Ryan")
# Delivery style — makes TTS sound conversational instead of a flat read-out.
INSTRUCT = os.environ.get("REGENT_SPEECH_INSTRUCT", "Speak naturally and conversationally.")
if "cuda" not in DEVICE:  # use all cores so CPU inference isn't single-threaded slow
    torch.set_num_threads(os.cpu_count() or 4)


def _tts_kwargs(language: str) -> dict:
    """Shared generate_custom_voice args (speaker + conversational instruct)."""
    kw = {"language": language, "speaker": SPEAKER}
    if INSTRUCT:
        kw["instruct"] = INSTRUCT
    return kw


app = FastAPI(title="regent-local-speech")
_asr = None  # lazy: load on first use (multi-GB), so the server starts instantly
_tts = None
# Guards the lazy loads: warm-up (background) and a first request can race
# otherwise, double-loading multi-GB models. Double-checked locking keeps the
# fast path lock-free once loaded.
_load_lock = threading.Lock()


def _load_asr():
    global _asr
    if _asr is None:
        with _load_lock:
            if _asr is None:
                from qwen_asr import Qwen3ASRModel  # pip install qwen-asr

                _asr = Qwen3ASRModel.from_pretrained(
                    str(ASR_DIR), dtype=DTYPE, device_map=DEVICE, max_new_tokens=256
                )
    return _asr


def _load_tts():
    global _tts
    if _tts is None:
        with _load_lock:
            if _tts is None:
                from qwen_tts import Qwen3TTSModel  # pip install qwen-tts

                # sdpa (built into torch) instead of flash_attention_2 — fast on
                # GPU, no extra flash-attn package to install/crash on.
                _tts = Qwen3TTSModel.from_pretrained(
                    str(TTS_DIR), device_map=DEVICE, dtype=DTYPE, attn_implementation="sdpa"
                )
    return _tts


def _warm_models() -> None:
    """Pre-load ASR+TTS off the request path so the first call isn't a 10-30s
    cold-load cliff (the models load regardless; this just front-loads it at
    startup). Best-effort — a failure here just falls back to lazy load."""
    try:
        if ASR_DIR.is_dir():
            _load_asr()
        if TTS_DIR.is_dir():
            _load_tts()
        print("  ✓ models warm — the first call won't pay the load cost")
    except Exception as e:  # noqa: BLE001 — warming is best-effort
        print(f"  ⚠ model warm-up failed (will load on first use): {e}")


def _transcript_text(results) -> str:
    """qwen_asr returns a list/dict/obj depending on version — get the text out."""
    if isinstance(results, str):
        return results
    if isinstance(results, list) and results:
        results = results[0]
    if isinstance(results, dict):
        return results.get("text", "")
    return getattr(results, "text", str(results))


@app.get("/health")
@app.get("/v1/models")
def health():
    return {"asr": ASR_DIR.is_dir(), "tts": TTS_DIR.is_dir(), "device": DEVICE, "models_dir": str(MODELS_DIR)}


@app.post("/v1/audio/transcriptions")
async def transcriptions(file: UploadFile, model: str = Form(default="")):
    """Regent posts the voice note's bytes (e.g. voice.ogg) as `file`."""
    data = await file.read()
    # Write to a temp file so the model's own decoder handles ogg/opus/wav/m4a.
    suffix = Path(file.filename or "audio.ogg").suffix or ".ogg"
    with tempfile.NamedTemporaryFile(suffix=suffix, delete=False) as tmp:
        tmp.write(data)
        path = tmp.name
    try:
        results = _load_asr().transcribe(audio=path, language=None)
        return {"text": _transcript_text(results).strip()}
    except Exception as e:  # never 500 the caller — return a clear error
        return JSONResponse({"error": f"ASR failed: {e}"}, status_code=500)
    finally:
        try:
            os.unlink(path)
        except OSError:
            pass


@app.post("/v1/audio/speech")
async def speech(request: Request):
    """Regent posts JSON {model, input, voice, response_format}; reply with audio bytes."""
    body = await request.json()
    text = (body.get("input") or "").strip()
    fmt = (body.get("response_format") or "wav").lower()
    if not text:
        return JSONResponse({"error": "empty input"}, status_code=400)
    try:
        wavs, sr = _load_tts().generate_custom_voice(text=text, **_tts_kwargs(LANG))
    except Exception as e:
        return JSONResponse({"error": f"TTS failed: {e}"}, status_code=500)
    audio = np.asarray(wavs[0] if isinstance(wavs, (list, tuple)) else wavs, dtype="float32")
    buf = io.BytesIO()
    try:  # Opus for Telegram voice bubbles; fall back to WAV if libsndfile can't.
        if fmt in ("opus", "ogg"):
            sf.write(buf, audio, sr, format="OGG", subtype="OPUS")
            media = "audio/ogg"
        else:
            raise ValueError("wav")
    except Exception:
        buf = io.BytesIO()
        sf.write(buf, audio, sr, format="WAV")
        media = "audio/wav"
    return Response(content=buf.getvalue(), media_type=media)


# Hands-free browser voice call (/call) — see web_call.py. Same dir, so a plain
# import works when run as `python scripts/local_speech_server.py`.
from web_call import register_call_routes  # noqa: E402

register_call_routes(app, _load_asr, _load_tts, _transcript_text, SPEAKER, INSTRUCT)


if __name__ == "__main__":
    print(f"regent-local-speech → http://localhost:8000  (device={DEVICE}, models={MODELS_DIR})")
    print(f"  voice call: http://localhost:8000/call")
    if "cuda" in DEVICE:
        print(f"  GPU: {torch.cuda.get_device_name(0)}")
    else:
        print("  ⚠ running on CPU (slow). For your RTX GPU, install a CUDA torch build:")
        print("    pip install --force-reinstall torch torchaudio --index-url https://download.pytorch.org/whl/cu124")
    if not ASR_DIR.is_dir() or not TTS_DIR.is_dir():
        print(f"  ⚠ weights missing — expected {ASR_DIR} and {TTS_DIR}")
    else:
        # Warm the models in the background so the server is reachable instantly
        # while ASR+TTS load — by the time the user opens /call and speaks, the
        # first turn skips the cold-load cliff.
        print("  warming models in the background…")
        threading.Thread(target=_warm_models, daemon=True).start()
    uvicorn.run(app, host="127.0.0.1", port=8000)
