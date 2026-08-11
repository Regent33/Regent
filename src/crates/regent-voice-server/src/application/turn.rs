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

mod synth;
pub use synth::warm_fillers;
use synth::{
    FILLER_REPEAT, FILLER_WAIT, FILLERS, KEEPALIVE_WAIT, MAX_FILLERS_PER_GAP, STALL_TIMEOUT, Synth,
    should_bridge_dead_air,
};

/// Warm the whisper graph so the FIRST real transcribe of the session doesn't
/// pay the cold start (ONNX graph build + first-inference allocation) on top of
/// the caller's wait — ASR is fully serial ahead of the model, so that penalty
/// lands squarely in the gap before Regent answers. The fillers are
/// pre-synthesized the same way; whisper was the one hot-path engine left cold.
/// Blocking (runs an inference) — call from `spawn_blocking`.
pub fn warm_asr(engines: &Engines) {
    let Some(asr) = engines.asr.clone() else {
        return;
    };
    // 0.3s of 16 kHz mono silence: whisper pads to its fixed window internally,
    // so this still exercises the full encoder/decoder. The (blank) transcript
    // is thrown away — we only want the graph hot.
    match asr.transcribe(&silent_wav_16k(4800), None) {
        Ok(_) => println!("[warm] whisper warmed"),
        Err(e) => println!("[warm] whisper warmup skipped ({e})"),
    }
}

/// A minimal PCM16 mono WAV of `samples` silent frames at 16 kHz, for warmup.
fn silent_wav_16k(samples: usize) -> Vec<u8> {
    const RATE: u32 = 16_000;
    let data_len = samples * 2;
    let mut w = Vec::with_capacity(44 + data_len);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&((36 + data_len) as u32).to_le_bytes());
    w.extend_from_slice(b"WAVE");
    w.extend_from_slice(b"fmt ");
    w.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    w.extend_from_slice(&1u16.to_le_bytes()); // PCM
    w.extend_from_slice(&1u16.to_le_bytes()); // mono
    w.extend_from_slice(&RATE.to_le_bytes());
    w.extend_from_slice(&(RATE * 2).to_le_bytes()); // byte rate
    w.extend_from_slice(&2u16.to_le_bytes()); // block align
    w.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    w.extend_from_slice(b"data");
    w.extend_from_slice(&(data_len as u32).to_le_bytes());
    w.resize(44 + data_len, 0); // silence
    w
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
            "[turn] gated ({reason}): peak_rms={:.4} floor_rms={:.4} voiced={:.2}s — no ASR",
            stats.peak_rms, stats.floor_rms, stats.voiced_secs
        );
        emit(json!({"noise": {"reason": reason}})).await;
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
        println!("[turn] asr={:.2}s · no speech", t_asr.as_secs_f32());
        emit(json!({"noise": {"reason": "empty_asr"}})).await;
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
        emit(json!({"noise": {"reason": "asr_hallucination"}})).await;
        return;
    }
    run_agent_turn(deps, heard, out, t0, t_asr).await;
}

/// Typed Butler input joins the same agent/TTS/diagram stream after the ASR
/// boundary, so keyboard and microphone turns share one session and behavior.
pub async fn run_text_turn(deps: TurnDeps, text: String, out: mpsc::Sender<String>) {
    let heard = text.trim().to_owned();
    if heard.is_empty() {
        out.send(json!({"error": "typed message is empty"}).to_string())
            .await
            .ok();
        return;
    }
    let t0 = Instant::now();
    run_agent_turn(deps, heard, out, t0, Duration::ZERO).await;
}

async fn run_agent_turn(
    deps: TurnDeps,
    heard: String,
    out: mpsc::Sender<String>,
    t0: Instant,
    t_asr: Duration,
) {
    let emit = |line: serde_json::Value| {
        let out = out.clone();
        async move {
            out.send(line.to_string()).await.ok();
        }
    };
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
        spoken_sentences: 0,
    };
    let mut splitter = SentenceSplitter::new();
    // Gate ```fenced``` spans (e.g. an appended `present` diagram spec) out of
    // the spoken stream; `full` still keeps everything for the client to parse.
    let mut gate = FenceGate::new();
    let mut full = String::new();
    let mut t_first_tok: Option<Duration> = None;
    // When the diagram spec reached the client. "The diagram takes a while" was
    // reported twice and argued about from theories both times, because the turn
    // line timed speech and nothing else — there was no number for the picture.
    // Measured from t0 like `first_audio`, so the two are directly comparable and
    // the answer to "did the diagram beat the voice?" is a subtraction.
    let mut t_spec: Option<Duration> = None;
    let mut bridged_dead_air = false;
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
        // so bridge the silence with SPOKEN lines: the first at FILLER_WAIT (or
        // the first keepalive tick mid-reply), then one every ~FILLER_REPEAT
        // while the gap continues, capped per gap — the old single filler per
        // turn left minutes of tool work in dead air. Between fillers, a silent
        // `keepalive` line every KEEPALIVE_WAIT keeps the client's hung-turn
        // watchdog fed (it resets on any streamed line). End the turn only
        // after STALL_TIMEOUT of true continuous silence (deacon hung/dropped).
        let mut silent = Duration::ZERO;
        let mut fillers_spoken = 0usize;
        let next = loop {
            let waiting_first = t_first_tok.is_none() && fillers_spoken == 0;
            let wait = if waiting_first {
                FILLER_WAIT
            } else {
                KEEPALIVE_WAIT
            };
            match tokio::time::timeout(wait, drx.recv()).await {
                Ok(d) => break d,
                Err(_) => {
                    silent += wait;
                    if silent >= STALL_TIMEOUT {
                        break None; // a real stall — end the turn
                    }
                    let due = FILLER_WAIT + FILLER_REPEAT * fillers_spoken as u32;
                    if fillers_spoken < MAX_FILLERS_PER_GAP && silent >= due {
                        // Pre-synthesized when the warm cache is in (instant);
                        // live TTS only before the warmup finished.
                        let i = rand::random::<u32>() as usize % FILLERS.len();
                        synth.filler(i, FILLERS[i]).await;
                        fillers_spoken += 1;
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
        // Tokens are arriving, so the silence-gap fillers above stay quiet — but
        // the CALLER still hears nothing until the first sentence closes and is
        // synthesized. Bridge that specific gap once, from the warm cache.
        if should_bridge_dead_air(synth.first_audio, t0.elapsed(), bridged_dead_air) {
            let i = rand::random::<u32>() as usize % FILLERS.len();
            synth.filler(i, FILLERS[i]).await;
            bridged_dead_air = true;
        }
        full.push_str(&delta);
        // Speak only the un-fenced portion; the fenced spec is dropped here so
        // it never reaches TTS, while `full` (sent to the client) keeps it.
        let speakable = gate.push(&delta);
        // A butler reply LEADS with its diagram spec, and a fenced span yields
        // no speakable text — so the loop below never runs while it streams and
        // no `reply` line goes out. The client could not draw the diagram until
        // a whole spoken sentence had also arrived and been split, which is
        // what made the explainer diagram feel slow (and, because the first
        // audio waits on the diagram being visible, delayed the voice too).
        // The spec is complete the moment its fence closes — send it then.
        if gate.closed_fence() {
            t_spec.get_or_insert_with(|| t0.elapsed());
            emit(json!({"reply": full})).await;
        }
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
        // `null` on a turn with no diagram, which is most of them — an absent
        // number here means "none was drawn", never "it was instant".
        "spec": t_spec.map(round2),
        "total": round2(t0.elapsed()),
    });
    println!(
        "[turn] asr={}s brain_ttft={}s first_audio={:?} spec={:?} total={}s",
        timing["asr"],
        timing["brain_ttft"],
        synth.first_audio.map(round2),
        t_spec.map(round2),
        timing["total"]
    );
    emit(json!({"timing": timing})).await;
}

fn round2(d: Duration) -> f64 {
    (d.as_secs_f64() * 100.0).round() / 100.0
}
