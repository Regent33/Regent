//! One `/call/turn`: mic WAV → ASR → agent stream → per-sentence TTS, emitted
//! as NDJSON lines (`heard` → `reply` updates → one `audio` per sentence →
//! `timing`). The voice starts after sentence 1 while the model is still
//! writing; a slow first token is bridged with one spoken filler line.

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
    emit(json!({"heard": heard})).await;
    if heard.is_empty() {
        println!("[turn] asr={:.2}s · no speech", t_asr.as_secs_f32());
        return; // VAD blip — nothing said
    }

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
    let mut full = String::new();
    let mut t_first_tok: Option<Duration> = None;
    let mut filled = false;
    loop {
        let waiting_first = t_first_tok.is_none() && !filled;
        let next = if waiting_first {
            match tokio::time::timeout(FILLER_WAIT, drx.recv()).await {
                Ok(d) => d,
                Err(_) => {
                    // Slow first token → bridge the silence once, keep waiting.
                    filled = true;
                    let pick = FILLERS[rand::random::<u32>() as usize % FILLERS.len()];
                    synth.sentence(pick).await;
                    continue;
                }
            }
        } else {
            match tokio::time::timeout(Duration::from_secs(180), drx.recv()).await {
                Ok(d) => d,
                Err(_) => break, // a real stall — stop rather than hang the call
            }
        };
        let Some(delta) = next else { break };
        if t_first_tok.is_none() {
            t_first_tok = Some(t0.elapsed());
        }
        full.push_str(&delta);
        for sentence in splitter.push(&delta) {
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
