"""web_call — a hands-free local voice call in the browser.

Mic → local Qwen3 ASR → Regent's model → local Qwen3 TTS → speaker, turn by turn.
Speech is fully local (no API key); the reply model uses REGENT_BRAIN_* / REGENT_*
env and falls back to an echo so the call still works with nothing configured.

Controls: language (ASR+TTS) and talk speed (time-stretch, pitch preserved).
Latency on CPU is dominated by the 1.7B models — a GPU is the real fix; we keep
replies short (system prompt) and run ASR/TTS in-process (no HTTP hop) to help.
"""
from __future__ import annotations

import base64
import io
import os
import tempfile

import numpy as np
import requests
import soundfile as sf
from fastapi import Request
from fastapi.responses import HTMLResponse, JSONResponse, Response

try:  # speed control without changing pitch
    import librosa
except Exception:  # noqa: BLE001 — optional
    librosa = None

BRAIN_URL = (
    os.environ.get("REGENT_BRAIN_BASE_URL")
    or os.environ.get("REGENT_BASE_URL")
    or "https://openrouter.ai/api/v1"
).rstrip("/")
BRAIN_KEY = os.environ.get("REGENT_BRAIN_API_KEY") or os.environ.get("REGENT_API_KEY", "")
BRAIN_MODEL = os.environ.get("REGENT_BRAIN_MODEL") or os.environ.get("REGENT_MODEL", "")
SYSTEM = (
    "You are Regent on a live voice call. Reply in one or two short, natural spoken "
    "sentences. No lists, no markdown, no emoji — it will be read aloud."
)


def _brain_reply(text: str) -> str:
    if not (BRAIN_KEY and BRAIN_MODEL):
        return f"I heard you say: {text}"  # echo — the call works with no model set
    try:
        r = requests.post(
            f"{BRAIN_URL}/chat/completions",
            headers={"Authorization": f"Bearer {BRAIN_KEY}"},
            json={
                "model": BRAIN_MODEL,
                "messages": [
                    {"role": "system", "content": SYSTEM},
                    {"role": "user", "content": text},
                ],
            },
            timeout=60,
        )
        return r.json()["choices"][0]["message"]["content"].strip()
    except Exception as e:  # noqa: BLE001 — surface it in the call instead of 500ing
        return f"(brain error: {e})"


def register_call_routes(app, load_asr, load_tts, transcript_text, speaker, instruct=""):
    @app.get("/", response_class=HTMLResponse)
    def index():
        return INDEX_HTML

    @app.get("/call", response_class=HTMLResponse)
    def call_page():
        return CALL_HTML

    @app.post("/call/turn")
    async def call_turn(request: Request):
        lang = request.query_params.get("language") or None
        try:
            speed = float(request.query_params.get("speed", "1.0"))
        except ValueError:
            speed = 1.0
        with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as tmp:
            tmp.write(await request.body())
            path = tmp.name
        try:
            heard = transcript_text(load_asr().transcribe(audio=path, language=lang)).strip()
        except Exception as e:  # noqa: BLE001
            return JSONResponse({"error": f"ASR: {e}"}, status_code=500)
        finally:
            try:
                os.unlink(path)
            except OSError:
                pass
        if not heard:
            return JSONResponse({"heard": "", "reply": "", "audio": ""})  # VAD blip
        reply = _brain_reply(heard)
        tts_kw = {"language": (lang or "Auto"), "speaker": speaker}
        if instruct:
            tts_kw["instruct"] = instruct  # conversational delivery
        try:
            wavs, sr = load_tts().generate_custom_voice(text=reply, **tts_kw)
        except Exception as e:  # noqa: BLE001
            return JSONResponse({"heard": heard, "reply": reply, "error": f"TTS: {e}"})
        audio = np.asarray(wavs[0] if isinstance(wavs, (list, tuple)) else wavs, dtype="float32")
        if librosa is not None and abs(speed - 1.0) > 0.01:
            audio = librosa.effects.time_stretch(audio, rate=speed)  # rate>1 = faster
        buf = io.BytesIO()
        sf.write(buf, audio, sr, format="WAV")
        return JSONResponse(
            {"heard": heard, "reply": reply, "audio": base64.b64encode(buf.getvalue()).decode()}
        )


# Status landing page (localhost:8000) — health + a quick type-to-speak box + a
# link into the call. ponytail: inline HTML, no template engine.
INDEX_HTML = """<!doctype html><html><head><meta charset=utf-8><title>Regent local speech</title>
<style>body{font-family:system-ui,sans-serif;max-width:640px;margin:48px auto;padding:0 16px;color:#1a1a1a}
h1{font-size:22px}h3{margin-top:28px}code{background:#f3f3f3;padding:1px 6px;border-radius:4px;font-size:13px}
input{width:100%;padding:9px;margin:8px 0;font-size:15px;box-sizing:border-box}
button{padding:9px 18px;font-size:15px;cursor:pointer}audio{width:100%;margin-top:10px}
.ok{color:#0a8a0a}.no{color:#c0392b}.muted{color:#777}</style></head><body>
<h1>&#9818; Regent local speech</h1>
<p id=stat class=muted>checking&hellip;</p>
<p><a href="/call">&#9742; Start a voice call &rarr;</a></p>
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

# Hands-free page: WebAudio mic capture + RMS VAD auto-stop, WAV-encode in the
# browser (no ffmpeg needed server-side), POST the utterance, play the reply, loop.
CALL_HTML = """<!doctype html><html><head><meta charset=utf-8><title>Call Regent</title>
<style>body{font-family:system-ui,sans-serif;max-width:560px;margin:48px auto;padding:0 16px;color:#1a1a1a}
h1{font-size:22px}#dot{font-size:13px;color:#777}button{padding:10px 20px;font-size:16px;cursor:pointer}
label{font-size:14px;color:#555;margin-right:6px}select,input{font-size:14px;margin-right:18px}
.row{margin:14px 0}.b{font-weight:600}.you{color:#0a66c2}.re{color:#0a8a0a}</style></head><body>
<h1>&#9818; Call Regent <span style=font-size:13px;color:#999>(local)</span></h1>
<div class=row>
 <label>Language</label><select id=lang>
  <option value="">Auto</option><option>English</option><option>Chinese</option>
  <option>Japanese</option><option>Korean</option><option>Spanish</option></select>
 <label>Talk speed</label><input id=speed type=range min=0.7 max=1.4 step=0.05 value=1>
 <span id=sv>1.0&times;</span></div>
<div class=row><button id=go onclick=toggle()>Start call</button> <span id=dot>idle</span></div>
<div class=row><span class=b>You:</span> <span id=heard class=you></span></div>
<div class=row><span class=b>Regent:</span> <span id=reply class=re></span></div>
<p style=font-size:12px;color:#999>First reply loads the model (slow on CPU). Just talk; it
 auto-sends when you pause. Echo cancellation is on, but headphones help.</p>
<script>
const DST=16000; let ac,stream,proc,buf=[],spk=false,sil=0,busy=false,on=false;
speed.oninput=()=>sv.textContent=(+speed.value).toFixed(2)+'×';
function st(t){dot.textContent=t}
async function toggle(){ if(on){stop();return}
 stream=await navigator.mediaDevices.getUserMedia({audio:{channelCount:1,echoCancellation:true,noiseSuppression:true}});
 ac=new AudioContext(); const s=ac.createMediaStreamSource(stream);
 proc=ac.createScriptProcessor(4096,1,1); s.connect(proc); proc.connect(ac.destination);
 proc.onaudioprocess=e=>{ if(busy)return; const d=e.inputBuffer.getChannelData(0);
  let r=0; for(let i=0;i<d.length;i++)r+=d[i]*d[i]; r=Math.sqrt(r/d.length);
  if(r>0.015){spk=true;sil=0;buf.push(new Float32Array(d));}
  else if(spk){sil++;buf.push(new Float32Array(d));
   if(sil>6){spk=false;sil=0;const u=buf;buf=[];send(u,ac.sampleRate);}}};
 on=true;go.textContent='End call';st('listening… just talk');}
function stop(){on=false;go.textContent='Start call';st('idle');
 proc&&proc.disconnect();stream&&stream.getTracks().forEach(t=>t.stop());ac&&ac.close();}
async function send(frames,sr){ busy=true;st('thinking…');
 const r=await fetch('/call/turn?language='+encodeURIComponent(lang.value)+'&speed='+speed.value,
  {method:'POST',body:wav(frames,sr)});
 const j=await r.json();
 heard.textContent=j.heard||'(didn\\'t catch that)'; reply.textContent=j.reply||j.error||'';
 if(j.audio){const a=new Audio('data:audio/wav;base64,'+j.audio);
  a.onended=()=>{busy=false;if(on)st('listening…')}; a.play();} else busy=false;}
function wav(frames,sr){ let n=0; for(const f of frames)n+=f.length; const all=new Float32Array(n);
 let o=0; for(const f of frames){all.set(f,o);o+=f.length;}
 const ratio=sr/DST, len=Math.floor(all.length/ratio), pcm=new Int16Array(len);
 for(let i=0;i<len;i++){let v=all[Math.floor(i*ratio)]||0; v=Math.max(-1,Math.min(1,v)); pcm[i]=v<0?v*32768:v*32767;}
 const b=new ArrayBuffer(44+len*2), dv=new DataView(b);
 const W=(o,s)=>{for(let i=0;i<s.length;i++)dv.setUint8(o+i,s.charCodeAt(i))};
 W(0,'RIFF');dv.setUint32(4,36+len*2,true);W(8,'WAVE');W(12,'fmt ');dv.setUint32(16,16,true);
 dv.setUint16(20,1,true);dv.setUint16(22,1,true);dv.setUint32(24,DST,true);dv.setUint32(28,DST*2,true);
 dv.setUint16(32,2,true);dv.setUint16(34,16,true);W(36,'data');dv.setUint32(40,len*2,true);
 for(let i=0;i<len;i++)dv.setInt16(44+i*2,pcm[i],true); return b;}
</script></body></html>"""
