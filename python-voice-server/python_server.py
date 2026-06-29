#!/usr/bin/env python3
"""
Local OpenAI-compatible speech server for Regent's `local` voice provider.

Real-time speech stack (measured swap from the Qwen3-1.7B pair, which was
~70s/turn on an 8GB laptop GPU, to ~1-2s/turn):
  • ASR — faster-whisper (CTranslate2, int8) — ~0.2-0.6s on GPU, sub-second on CPU
  • TTS — Piper (ONNX) — faster-than-realtime on CPU, frees the GPU for ASR

Serves the two endpoints Regent's `OpenAiCompat` ASR/TTS clients call, plus the
hands-free browser call at /call:

    POST /v1/audio/transcriptions   (multipart: file, model)        -> {"text": ...}
    POST /v1/audio/speech           (json: input, voice, response_format) -> audio bytes

Quick start (use your real ML Python — per the about-profile, Python 3.14):
    pip install faster-whisper piper-tts soundfile
    python python-voice-server/python_server.py      # serves on http://localhost:8000
    # then open http://localhost:8000/call, or:
    regent voice setup --provider local              # base_url http://localhost:8000/v1

Env: REGENT_SPEECH_DEVICE=cpu|cuda (default: cuda if available),
REGENT_WHISPER_SIZE=tiny|base|small|medium|large-v3 (default small),
REGENT_PIPER_VOICE=<voice> (default en_US-lessac-medium),
REGENT_MODELS_DIR to override where the Piper voice is stored.
The Qwen3 weights under tts-asr-local-models, if present, are no longer used.
"""
from __future__ import annotations

import io
import os
import subprocess
import sys
import tempfile
import threading
import wave
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
HAS_CUDA = torch.cuda.is_available()
DEVICE = os.environ.get("REGENT_SPEECH_DEVICE") or ("cuda" if HAS_CUDA else "cpu")
WHISPER_SIZE = os.environ.get("REGENT_WHISPER_SIZE", "small")
PIPER_VOICE = os.environ.get("REGENT_PIPER_VOICE", "en_US-lessac-medium")
VOICES_DIR = MODELS_DIR / "piper-voices"

# Call-UI language label -> Whisper code (None = auto-detect).
_WHISPER_LANG = {"English": "en", "Chinese": "zh", "Japanese": "ja", "Korean": "ko", "Spanish": "es"}


def _wlang(language: str | None) -> str | None:
    if not language:
        return None
    return _WHISPER_LANG.get(language, language if len(language) == 2 else None)


class _FastASR:
    """faster-whisper wrapped to the `.transcribe(audio, language) -> str` shape
    the rest of the server (and web_call.py) already expects."""

    def __init__(self, model):
        self.model = model

    def transcribe(self, audio, language=None) -> str:
        segments, _ = self.model.transcribe(audio, language=_wlang(language), beam_size=1)
        return "".join(seg.text for seg in segments).strip()


class _FastTTS:
    """Piper wrapped to the `.generate_custom_voice(text) -> (audio, sr)` shape
    (extra kwargs like speaker/instruct are Qwen-only and ignored)."""

    def __init__(self, voice):
        self.voice = voice

    def generate_custom_voice(self, text, **_ignored):
        buf = io.BytesIO()
        with wave.open(buf, "wb") as wf:
            self.voice.synthesize_wav(text, wf)
        buf.seek(0)
        audio, sr = sf.read(buf, dtype="float32")
        return audio, sr


def _ensure_piper_voice() -> str:
    """Path to the Piper .onnx, downloading the voice on first run if missing."""
    VOICES_DIR.mkdir(parents=True, exist_ok=True)
    onnx = VOICES_DIR / f"{PIPER_VOICE}.onnx"
    if not onnx.exists():
        print(f"  downloading Piper voice {PIPER_VOICE}…", flush=True)
        subprocess.run(
            [sys.executable, "-m", "piper.download_voices", PIPER_VOICE, "--download-dir", str(VOICES_DIR)],
            check=True,
        )
    return str(onnx)


app = FastAPI(title="regent-local-speech")
_asr = None  # lazy: load on first use, so the server starts instantly
_tts = None
# Guards the lazy loads: warm-up (background) and a first request can race
# otherwise. Double-checked locking keeps the fast path lock-free once loaded.
_load_lock = threading.Lock()


def _load_asr():
    global _asr
    if _asr is None:
        with _load_lock:
            if _asr is None:
                from faster_whisper import WhisperModel  # pip install faster-whisper

                compute = "int8_float16" if DEVICE == "cuda" else "int8"
                _asr = _FastASR(WhisperModel(WHISPER_SIZE, device=DEVICE, compute_type=compute))
    return _asr


def _load_tts():
    global _tts
    if _tts is None:
        with _load_lock:
            if _tts is None:
                from piper import PiperVoice  # pip install piper-tts

                _tts = _FastTTS(PiperVoice.load(_ensure_piper_voice()))
    return _tts


def _warm_models() -> None:
    """Pre-load ASR+TTS off the request path so the first call isn't a cold-load
    cliff. Best-effort — a failure here just falls back to lazy load."""
    try:
        _load_asr()
        _load_tts()
        print("  ✓ models warm — the first call won't pay the load cost", flush=True)
    except Exception as e:  # noqa: BLE001 — warming is best-effort
        print(f"  ⚠ model warm-up failed (will load on first use): {e}", flush=True)


def _transcript_text(results) -> str:
    """ASR adapter returns a str; kept tolerant for any wrapper shape."""
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
    return {
        "engine": "faster-whisper+piper",
        "asr": WHISPER_SIZE,
        "tts": PIPER_VOICE,
        "device": DEVICE,
        "models_dir": str(MODELS_DIR),
    }


@app.post("/v1/audio/transcriptions")
async def transcriptions(file: UploadFile, model: str = Form(default="")):
    """Regent posts the voice note's bytes (e.g. voice.ogg) as `file`."""
    data = await file.read()
    suffix = Path(file.filename or "audio.ogg").suffix or ".ogg"
    with tempfile.NamedTemporaryFile(suffix=suffix, delete=False) as tmp:
        tmp.write(data)
        path = tmp.name
    try:
        return {"text": _transcript_text(_load_asr().transcribe(audio=path, language=None)).strip()}
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
        wavs, sr = _load_tts().generate_custom_voice(text=text)
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
# import works when run as `python python_server.py`.
from web_call import register_call_routes  # noqa: E402

register_call_routes(app, _load_asr, _load_tts, _transcript_text, "", "", DEVICE)


if __name__ == "__main__":
    print(f"regent-local-speech → http://localhost:8000  (device={DEVICE})")
    print(f"  ASR: faster-whisper '{WHISPER_SIZE}'  ·  TTS: Piper '{PIPER_VOICE}'")
    print("  voice call: http://localhost:8000/call")
    if DEVICE == "cuda":
        print(f"  GPU: {torch.cuda.get_device_name(0)}")
    else:
        print("  running on CPU (still real-time with this stack)")
    print("  warming models in the background…")
    threading.Thread(target=_warm_models, daemon=True).start()
    uvicorn.run(app, host="127.0.0.1", port=8000)
