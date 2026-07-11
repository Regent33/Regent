//! One `/call/turn`: mic WAV → ASR → agent stream → per-sentence TTS, emitted
//! as NDJSON lines (`heard` → `reply` updates → one `audio` per sentence →
//! `timing`). The voice starts after sentence 1 while the model is still
//! writing; a slow first token is bridged with one spoken filler line.

use crate::domain::fence::FenceGate;
use crate::domain::sentences::SentenceSplitter;
use crate::domain::speakable::{strip_markdown, strip_spoken};
use crate::infra::deacon::DeaconRpc;
use crate::infra::engines::Engines;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// If the first reply token takes longer than this (tools running, model
/// thinking), speak one short line so the call isn't dead air. 2.5s — long
/// enough that quick replies never trigger it (it played on almost every
/// turn at 1.6s, which read as canned).
const FILLER_WAIT: Duration = Duration::from_millis(2500);
/// While the brain is still working (long think / tool calls) it streams nothing,
/// so emit a silent `keepalive` line this often. The client resets its hung-turn
/// watchdog on any streamed line, so a legit long turn is never mistaken for a
/// dead one. Must stay well under the client's ~20s silence threshold.
const KEEPALIVE_WAIT: Duration = Duration::from_secs(8);
/// Give up on a turn only after this much *continuous* brain silence (a real
/// stall — deacon hung or dropped), rather than keepalive-ing forever. 10 min:
/// deep context searches legitimately stream nothing for minutes (keepalives
/// bridge the client), and 3 min was ending real turns early.
const STALL_TIMEOUT: Duration = Duration::from_secs(600);
const FILLERS: [&str; 8] = [
    "Just a sec.",
    "One moment.",
    "On it.",
    "Let me check.",
    "Hmm, let me see.",
    "Give me a second.",
    "Looking now.",
    "Okay, hold on.",
];

/// Pre-synthesized filler WAVs (base64), index-aligned with [`FILLERS`].
/// Warmed once in the background after the engines load, so speaking a filler
/// costs zero TTS latency — exactly the moment the call is bridging dead air.
/// Empty until warm; the filler path falls back to live synthesis.
static FILLER_CACHE: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();

/// Synthesizes every filler line into [`FILLER_CACHE`]. Blocking (TTS) — call
/// from `spawn_blocking`. All-or-nothing: a partial cache would bias the
/// random pick toward the cached lines.
pub fn warm_fillers(engines: &Engines) {
    let Some(tts) = engines.tts.clone() else {
        return;
    };
    let mut cache = Vec::with_capacity(FILLERS.len());
    for text in FILLERS {
        match tts.synthesize(text) {
            Ok(audio) => cache.push(B64.encode(regent_speech::wav::encode(&audio))),
            Err(e) => {
                println!("[warm] filler pre-synthesis failed ({e}) — fillers stay live");
                return;
            }
        }
    }
    let _ = FILLER_CACHE.set(cache);
    println!("[warm] {} filler lines pre-synthesized", FILLERS.len());
}

pub struct TurnDeps {
    pub engines: Engines,
    pub deacon: Option<Arc<DeaconRpc>>,
    /// Why the agent is off (spoken once in echo mode so the caller isn't
    /// left guessing what "I heard you say" means).
    pub agent_note: String,
}

/// Run one turn, sending NDJSON lines (without trailing newline) into `out`.
pub async fn run_turn(
    deps: TurnDeps,
    body: Vec<u8>,
    language: Option<String>,
    out: mpsc::Sender<String>,
) {
    let emit = |line: serde_json::Value| {
        let out = out.clone();
        async move {
            out.send(line.to_string()).await.ok();
        }
    };
    let t0 = Instant::now();
    let Some(asr) = deps.engines.asr.clone() else {
        emit(json!({"error": format!("ASR: {}", deps.engines.note)})).await;
        return;
    };

    // Server-side VAD safety net (see domain::vad). The client already
    // energy-gates the mic, but its VAD runs on the browser main thread and a
    // noise burst can clip past it — and whisper then hallucinates words from
    // that room noise, which drives a phantom agent turn (the reported "picks
    // up noise" bug). Decode the PCM once and, if it's near-silence or a blip
    // too short to be speech, drop it BEFORE whisper runs (also saves the
    // wasted ASR latency). Parse failures fall through to ASR, which reports
    // them with a clear message.
    let vad = crate::domain::vad::VadConfig::from_env();
    let stats = crate::domain::wav::parse_pcm16_mono(&body)
        .ok()
        .map(|(rate, samples)| crate::domain::vad::analyze(&samples, rate, vad.min_rms));
    if let Some(stats) = &stats
        && let Some(reason) = crate::domain::vad::pre_asr_reject(stats, &vad)
    {
        println!(
            "[turn] gated ({reason}): peak_rms={:.4} voiced={:.2}s — no ASR",
            stats.peak_rms, stats.voiced_secs
        );
        return; // stay listening; don't flash a spurious "heard"
    }

    let lang = language.clone();
    let heard = tokio::task::spawn_blocking(move || asr.transcribe(&body, lang.as_deref()))
        .await
        .unwrap_or_else(|e| Err(e.to_string()));
    let heard = match heard {
        Ok(h) => h.trim().to_owned(),
        Err(e) => {
            emit(json!({"error": format!("ASR: {e}")})).await;
            return;
        }
    };
    let t_asr = t0.elapsed();
    if heard.is_empty() {
        emit(json!({"heard": heard})).await;
        println!("[turn] asr={:.2}s · no speech", t_asr.as_secs_f32());
        return; // VAD blip — nothing said
    }
    // Post-ASR net: quiet audio + a stock whisper silence-phrase = a
    // hallucination, not a turn. Drop it rather than answer phantom noise.
    if let Some(stats) = &stats
        && crate::domain::vad::is_noise_hallucination(&heard, stats, &vad)
    {
        println!(
            "[turn] dropped likely hallucination {heard:?}: voiced_rms={:.4}",
            stats.voiced_rms
        );
        return;
    }
    emit(json!({"heard": heard})).await;

    // A missing TTS engine would let the turn stream reply text but no audio,
    // silently — surface it once up front (mirroring the ASR-missing path) so
    // the caller learns why instead of getting dead air.
    if deps.engines.tts.is_none() {
        emit(json!({"error": "TTS unavailable — replying in text only (check /health)."})).await;
    }

    // The agent (tools/memory via the deacon) streamed token-by-token; with no
    // deacon the call still answers (echo) and SAYS why, so "I heard you say"
    // is never a mystery.
    let (dtx, mut drx) = mpsc::unbounded_channel();
    match deps.deacon.clone() {
        Some(rpc) => {
            let text = heard.clone();
            tokio::spawn(async move { rpc.stream_turn(&text, dtx).await });
        }
        None => {
            dtx.send(format!(
                "I heard you say: {heard}. My agent brain isn't connected right now — {}.",
                deps.agent_note
            ))
            .ok();
        }
    }

    let mut synth = Synth {
        engines: deps.engines.clone(),
        out: out.clone(),
        idx: 0,
        first_audio: None,
        t0,
    };
    let mut splitter = SentenceSplitter::new();
    // Gate ```fenced``` spans (e.g. an appended `present` diagram spec) out of
    // the spoken stream; `full` still keeps everything for the client to parse.
    let mut gate = FenceGate::new();
    let mut full = String::new();
    let mut t_first_tok: Option<Duration> = None;
    let mut filled = false;
    loop {
        // Clean barge-in / hang-up: when the caller talks over Regent (or ends
        // the call), the client aborts the fetch, so the response stream — and
        // this channel's receiver — is dropped. Stop the moment that happens
        // instead of running the abandoned agent + TTS to completion, which
        // would burn CPU and delay the real next turn. (The deacon's own turn
        // is already cancelled by the next turn's `turn.interrupt`.)
        if out.is_closed() {
            println!("[turn] caller disconnected (barge-in / hang-up) — stopping");
            return;
        }
        // Wait for the next brain delta. A long think / tool call streams nothing,
        // so bridge the silence: one spoken filler for the first gap, then a silent
        // `keepalive` line every KEEPALIVE_WAIT so the client's hung-turn watchdog
        // sees the server is alive (it resets on any streamed line). End the turn
        // only after STALL_TIMEOUT of true continuous silence (deacon hung/dropped).
        let mut silent = Duration::ZERO;
        let next = loop {
            let waiting_first = t_first_tok.is_none() && !filled;
            let wait = if waiting_first {
                FILLER_WAIT
            } else {
                KEEPALIVE_WAIT
            };
            match tokio::time::timeout(wait, drx.recv()).await {
                Ok(d) => break d,
                Err(_) => {
                    silent += wait;
                    if t_first_tok.is_none() && !filled {
                        // Slow first token → one spoken filler bridges the gap.
                        // Pre-synthesized when the warm cache is in (instant);
                        // live TTS only before the warmup finished.
                        filled = true;
                        let i = rand::random::<u32>() as usize % FILLERS.len();
                        match FILLER_CACHE.get() {
                            Some(cache) => synth.cached(&cache[i]).await,
                            None => synth.sentence(FILLERS[i]).await,
                        }
                    } else if silent >= STALL_TIMEOUT {
                        break None; // a real stall — end the turn
                    } else {
                        // Still working — keep the client's watchdog alive.
                        emit(json!({"keepalive": true})).await;
                    }
                }
            }
        };
        let Some(delta) = next else { break };
        if t_first_tok.is_none() {
            t_first_tok = Some(t0.elapsed());
        }
        full.push_str(&delta);
        // Speak only the un-fenced portion; the fenced spec is dropped here so
        // it never reaches TTS, while `full` (sent to the client) keeps it.
        let speakable = gate.push(&delta);
        for sentence in splitter.push(&speakable) {
            if out.is_closed() {
                return; // barged over mid-reply — don't synth the rest
            }
            // Update the transcript per SENTENCE, not per token — per-token
            // floods the client and degrades its main-thread VAD.
            emit(json!({"reply": full})).await;
            synth.sentence(&sentence).await;
        }
    }
    emit(json!({"reply": full})).await;
    if let Some(tail) = splitter.flush() {
        synth.sentence(&tail).await;
    }

    let timing = json!({
        "asr": round2(t_asr),
        "brain_ttft": round2(t_first_tok.unwrap_or_else(|| t0.elapsed()) - t_asr),
        "first_audio": synth.first_audio.map(round2),
        "total": round2(t0.elapsed()),
    });
    println!(
        "[turn] asr={}s brain_ttft={}s first_audio={:?} total={}s",
        timing["asr"],
        timing["brain_ttft"],
        synth.first_audio.map(round2),
        timing["total"]
    );
    emit(json!({"timing": timing})).await;
}

fn round2(d: Duration) -> f64 {
    (d.as_secs_f64() * 100.0).round() / 100.0
}

/// Per-sentence synthesis: sanitize → TTS → WAV → base64 → one `audio` line.
struct Synth {
    engines: Engines,
    out: mpsc::Sender<String>,
    idx: u32,
    first_audio: Option<Duration>,
    t0: Instant,
}

impl Synth {
    /// Emits an already-encoded WAV (the filler cache) — no TTS in the path.
    async fn cached(&mut self, b64: &str) {
        if self.first_audio.is_none() {
            self.first_audio = Some(self.t0.elapsed());
        }
        let i = self.idx;
        self.idx += 1;
        self.out
            .send(json!({"audio": b64, "i": i}).to_string())
            .await
            .ok();
    }

    async fn sentence(&mut self, text: &str) {
        let clean = strip_markdown(&strip_spoken(text));
        if clean.is_empty() {
            return;
        }
        let Some(tts) = self.engines.tts.clone() else {
            return;
        };
        let line = tokio::task::spawn_blocking(move || tts.synthesize(&clean))
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
        let line = match line {
            Ok(audio) => {
                let wav = regent_speech::wav::encode(&audio);
                if self.first_audio.is_none() {
                    self.first_audio = Some(self.t0.elapsed());
                }
                let i = self.idx;
                self.idx += 1;
                json!({"audio": B64.encode(wav), "i": i})
            }
            Err(e) => json!({"error": format!("TTS: {e}")}),
        };
        self.out.send(line.to_string()).await.ok();
    }
}
