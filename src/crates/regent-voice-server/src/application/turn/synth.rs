//! Filler phrases + the streaming TTS synthesizer for a call turn.
//! Split from `turn.rs` (file-size rule).

use super::*;

/// If the first reply token takes longer than this (tools running, model
/// thinking), speak one short line so the call isn't dead air. 2.5s — long
/// enough that quick replies never trigger it (it played on almost every
/// turn at 1.6s, which read as canned).
pub(super) const FILLER_WAIT: Duration = Duration::from_millis(2500);
/// While the brain is still working (long think / tool calls) it streams nothing,
/// so emit a silent `keepalive` line this often. The client resets its hung-turn
/// watchdog on any streamed line, so a legit long turn is never mistaken for a
/// dead one. Must stay well under the client's ~20s silence threshold.
pub(super) const KEEPALIVE_WAIT: Duration = Duration::from_secs(8);
/// Give up on a turn only after this much *continuous* brain silence (a real
/// stall — deacon hung or dropped), rather than keepalive-ing forever. 10 min:
/// deep context searches legitimately stream nothing for minutes (keepalives
/// bridge the client), and 3 min was ending real turns early.
pub(super) const STALL_TIMEOUT: Duration = Duration::from_secs(600);
pub(super) const FILLERS: [&str; 8] = [
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
pub(super) static FILLER_CACHE: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();

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

/// Per-sentence synthesis: sanitize → TTS → WAV → base64 → one `audio` line.
pub(super) struct Synth {
    pub(super) engines: Engines,
    pub(super) out: mpsc::Sender<String>,
    pub(super) idx: u32,
    pub(super) first_audio: Option<Duration>,
    pub(super) t0: Instant,
}

impl Synth {
    /// Emits an already-encoded WAV (the filler cache) — no TTS in the path.
    pub(super) async fn cached(&mut self, b64: &str) {
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

    pub(super) async fn sentence(&mut self, text: &str) {
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
