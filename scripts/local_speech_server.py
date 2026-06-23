#!/usr/bin/env python3
"""
Local OpenAI-compatible speech server for Regent's `local` voice provider.

Serves the two endpoints Regent's `OpenAiCompat` ASR/TTS clients call, against
the Qwen3 weights staged in ../tts-asr-local-models:

    POST /v1/audio/transcriptions   (multipart: file, model)        -> {"text": ...}
    POST /v1/audio/speech           (json: input, voice, response_format) -> audio bytes

Once it's running, point Regent at it:
    regent voice setup --provider local        # base_url http://localhost:8000/v1
    regent voice test

Run:
    pip install fastapi "uvicorn[standard]" soundfile transformers torch
    # plus the model's own deps (qwen_tts for TTS — see QwenLM/Qwen3-TTS)
    python scripts/local_speech_server.py        # serves on :8000

Use your real ML Python (per the about-profile, Python 3.14 at
pythoncore-3.14-64), NOT the PyManager stub.

NOTE: the HTTP contract below is exactly what Regent sends/expects — that part
is correct and tested against the Rust client. The two `# === MODEL ===` blocks
are the inference calls; fill them in per the Qwen3-ASR / Qwen3-TTS READMEs
(their exact loader/generate API is the one thing this scaffold can't pin down
for you). Everything around them is ready.
"""
from __future__ import annotations

import io
import os
from pathlib import Path

import soundfile as sf  # noqa: F401  (used in the TTS block)
import uvicorn
from fastapi import FastAPI, Form, Request, UploadFile
from fastapi.responses import JSONResponse, Response

MODELS_DIR = Path(os.environ.get("REGENT_MODELS_DIR", "tts-asr-local-models")).resolve()
ASR_DIR = MODELS_DIR / "Qwen3-ASR-1.7B"
TTS_DIR = MODELS_DIR / "Qwen3-TTS-12Hz-1.7B-CustomVoice"

app = FastAPI(title="regent-local-speech")

# Models are loaded lazily on first use so the server starts instantly and only
# pays the (multi-GB) load cost for the modality you actually call.
_asr = None
_tts = None


def _load_asr():
    global _asr
    if _asr is None:
        if not ASR_DIR.is_dir():
            raise FileNotFoundError(f"ASR weights not found at {ASR_DIR}")
        # === MODEL: load Qwen3-ASR-1.7B (see github.com/QwenLM/Qwen3-ASR) ===
        # e.g. transformers AutoModelForSpeechSeq2Seq / their toolkit:
        #   from transformers import pipeline
        #   _asr = pipeline("automatic-speech-recognition", model=str(ASR_DIR))
        raise NotImplementedError("wire Qwen3-ASR load+transcribe here")
    return _asr


def _load_tts():
    global _tts
    if _tts is None:
        if not TTS_DIR.is_dir():
            raise FileNotFoundError(f"TTS weights not found at {TTS_DIR}")
        # === MODEL: load Qwen3-TTS (see github.com/QwenLM/Qwen3-TTS) ===
        #   from qwen_tts import QwenTTS
        #   _tts = QwenTTS.from_pretrained(str(TTS_DIR))
        raise NotImplementedError("wire Qwen3-TTS load+synthesize here")
    return _tts


@app.get("/health")
@app.get("/v1/models")
def health():
    return {"asr": ASR_DIR.is_dir(), "tts": TTS_DIR.is_dir(), "models_dir": str(MODELS_DIR)}


@app.post("/v1/audio/transcriptions")
async def transcriptions(file: UploadFile, model: str = Form(default="")):
    """Regent sends the voice note's bytes (e.g. voice.ogg) as `file`."""
    audio_bytes = await file.read()
    try:
        asr = _load_asr()
    except (FileNotFoundError, NotImplementedError) as e:
        return JSONResponse({"error": str(e)}, status_code=501)
    # === MODEL: transcribe `audio_bytes` with `asr` -> str ===
    #   import soundfile as sf, io
    #   wav, sr = sf.read(io.BytesIO(audio_bytes))
    #   text = asr({"array": wav, "sampling_rate": sr})["text"]
    text = ""  # replace with the line above
    return {"text": text}


@app.post("/v1/audio/speech")
async def speech(request: Request):
    """Regent sends JSON {model, input, voice, response_format}; reply with audio bytes."""
    body = await request.json()
    text = body.get("input", "")
    response_format = body.get("response_format", "wav")
    try:
        tts = _load_tts()
    except (FileNotFoundError, NotImplementedError) as e:
        return JSONResponse({"error": str(e)}, status_code=501)
    # === MODEL: synthesize `text` with `tts` -> (wav: np.ndarray, sr: int) ===
    #   wav, sr = tts.synthesize(text, voice=body.get("voice"))
    #   buf = io.BytesIO(); sf.write(buf, wav, sr, format=response_format.upper()); data = buf.getvalue()
    data = b""  # replace with the lines above
    media = {"opus": "audio/ogg", "mp3": "audio/mpeg", "wav": "audio/wav"}.get(
        response_format, "application/octet-stream"
    )
    return Response(content=data, media_type=media)


if __name__ == "__main__":
    print(f"regent-local-speech → http://localhost:8000  (models: {MODELS_DIR})")
    uvicorn.run(app, host="127.0.0.1", port=8000)
