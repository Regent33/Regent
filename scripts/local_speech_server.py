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
from fastapi.responses import HTMLResponse, JSONResponse, Response

MODELS_DIR = Path(os.environ.get("REGENT_MODELS_DIR", "tts-asr-local-models")).resolve()
ASR_DIR = MODELS_DIR / "Qwen3-ASR-1.7B"
TTS_DIR = MODELS_DIR / "Qwen3-TTS-12Hz-1.7B-CustomVoice"
DEVICE = os.environ.get("REGENT_SPEECH_DEVICE") or ("cuda:0" if torch.cuda.is_available() else "cpu")
DTYPE = torch.bfloat16 if "cuda" in DEVICE else torch.float32
LANG = os.environ.get("REGENT_SPEECH_LANG", "English")
# CustomVoice requires a speaker; pick an English-native one. Others: Serena,
# Vivian, Uncle_Fu, Aiden, Ono_Anna, Sohee, Eric, Dylan (see the model README).
SPEAKER = os.environ.get("REGENT_SPEECH_SPEAKER", "Ryan")

app = FastAPI(title="regent-local-speech")
_asr = None  # lazy: load on first use (multi-GB), so the server starts instantly
_tts = None


def _load_asr():
    global _asr
    if _asr is None:
        from qwen_asr import Qwen3ASRModel  # pip install qwen-asr

        _asr = Qwen3ASRModel.from_pretrained(
            str(ASR_DIR), dtype=DTYPE, device_map=DEVICE, max_new_tokens=256
        )
    return _asr


def _load_tts():
    global _tts
    if _tts is None:
        from qwen_tts import Qwen3TTSModel  # pip install qwen-tts

        kw = {"device_map": DEVICE, "dtype": DTYPE}
        if "cuda" in DEVICE:
            kw["attn_implementation"] = "flash_attention_2"  # GPU-only
        _tts = Qwen3TTSModel.from_pretrained(str(TTS_DIR), **kw)
    return _tts


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


# A tiny status page + a "type text → hear it" box, so opening localhost:8000 in a
# browser shows something useful instead of a blank 404. ponytail: inline HTML, no
# template engine — it's one static page.
INDEX_HTML = """<!doctype html><html><head><meta charset=utf-8><title>Regent local speech</title>
<style>body{font-family:system-ui,sans-serif;max-width:640px;margin:48px auto;padding:0 16px;color:#1a1a1a}
h1{font-size:22px}h3{margin-top:28px}code{background:#f3f3f3;padding:1px 6px;border-radius:4px;font-size:13px}
input{width:100%;padding:9px;margin:8px 0;font-size:15px;box-sizing:border-box}
button{padding:9px 18px;font-size:15px;cursor:pointer}audio{width:100%;margin-top:10px}
.ok{color:#0a8a0a}.no{color:#c0392b}.muted{color:#777}</style></head><body>
<h1>&#9818; Regent local speech</h1>
<p id=stat class=muted>checking&hellip;</p>
<p class=muted>Endpoints: <code>POST /v1/audio/speech</code> &middot; <code>POST /v1/audio/transcriptions</code></p>
<h3>Try text&#8209;to&#8209;speech</h3>
<input id=t value="Hello from Regent." />
<button id=b onclick=say()>Speak</button>
<audio id=a controls></audio>
<script>
fetch('/health').then(r=>r.json()).then(d=>{stat.className='';stat.innerHTML=
 (d.asr&&d.tts?'<span class=ok>&#9679; ready</span>':'<span class=no>&#9679; weights missing</span>')
 +' &mdash; device <b>'+d.device+'</b>, models <code>'+d.models_dir+'</code>'})
 .catch(()=>{stat.className='no';stat.textContent='server unreachable'})
async function say(){b.disabled=true;a.removeAttribute('src');stat.className='muted';
 stat.textContent='synthesizing (first call loads the model &mdash; slow on CPU)…';
 try{const r=await fetch('/v1/audio/speech',{method:'POST',headers:{'content-type':'application/json'},
  body:JSON.stringify({input:t.value,response_format:'wav'})});
  if(!r.ok){stat.className='no';stat.textContent=await r.text();return}
  a.src=URL.createObjectURL(await r.blob());a.play();stat.className='ok';stat.textContent='done'}
 catch(e){stat.className='no';stat.textContent=String(e)}finally{b.disabled=false}}
</script></body></html>"""


@app.get("/", response_class=HTMLResponse)
def index():
    return INDEX_HTML


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
        wavs, sr = _load_tts().generate_custom_voice(text=text, language=LANG, speaker=SPEAKER)
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


if __name__ == "__main__":
    print(f"regent-local-speech → http://localhost:8000  (device={DEVICE}, models={MODELS_DIR})")
    if not ASR_DIR.is_dir() or not TTS_DIR.is_dir():
        print(f"  ⚠ weights missing — expected {ASR_DIR} and {TTS_DIR}")
    uvicorn.run(app, host="127.0.0.1", port=8000)
