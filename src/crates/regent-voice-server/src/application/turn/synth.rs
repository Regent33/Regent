//! Filler phrases + the streaming TTS synthesizer for a call turn.
//! Split from `turn.rs` (file-size rule).

use super::*;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// If the first reply token takes longer than this (tools running, model
/// thinking), speak one short line so the call isn't dead air. 2.5s — long
/// enough that quick replies never trigger it (it played on almost every
/// turn at 1.6s, which read as canned).
pub(super) const FILLER_WAIT: Duration = Duration::from_millis(2500);
/// While the brain is still working (long think / tool calls) it streams nothing,
/// so emit a silent `keepalive` line this often. The client resets its hung-turn
/// watchdog on any streamed line, so a legit long turn is never mistaken for a
/// dead one. Must stay well under the client's silence threshold.
pub(super) const KEEPALIVE_WAIT: Duration = Duration::from_secs(8);
/// Give up on a turn only after this much *continuous* brain silence (a real
/// stall — deacon hung or dropped), rather than keepalive-ing forever. 10 min:
/// deep context searches legitimately stream nothing for minutes (keepalives
/// bridge the client), and 3 min was ending real turns early.
pub(super) const STALL_TIMEOUT: Duration = Duration::from_secs(600);
/// Keep Butler conversational even when the model returns an essay. The full
/// reply still streams to the transcript; only this many content sentences
/// reach TTS before one concise on-screen handoff.
const MAX_SPOKEN_SENTENCES: u8 = 3;
const SPOKEN_HANDOFF: &str = "I've put the rest on screen.";
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

/// Encoded filler WAVs keyed by live voice profile and phrase index.
/// Startup warms the initial profile; settings changes populate their own cache.
static FILLER_CACHE: OnceLock<Mutex<HashMap<(String, usize), String>>> = OnceLock::new();

fn filler_cache() -> &'static Mutex<HashMap<(String, usize), String>> {
    FILLER_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Synthesizes every filler line into [`FILLER_CACHE`]. Blocking (TTS) — call
/// from `spawn_blocking`. All-or-nothing: a partial cache would bias the
/// random pick toward the cached lines.
pub fn warm_fillers(engines: &Engines) {
    let Some(tts) = engines.tts.clone() else {
        return;
    };
    let profile = tts.cache_key();
    let mut cache = Vec::with_capacity(FILLERS.len());
    for text in FILLERS {
        if tts.cache_key() != profile {
            println!("[warm] voice settings changed — abandoning stale filler warmup");
            return;
        }
        match tts.synthesize(text) {
            Ok(audio) => cache.push(B64.encode(regent_speech::wav::encode(&audio))),
            Err(e) => {
                println!("[warm] filler pre-synthesis failed ({e}) — fillers stay live");
                return;
            }
        }
    }
    if tts.cache_key() != profile {
        println!("[warm] voice settings changed — abandoning stale filler warmup");
        return;
    }
    filler_cache().lock().unwrap().extend(
        cache
            .into_iter()
            .enumerate()
            .map(|(i, audio)| ((profile.clone(), i), audio)),
    );
    println!("[warm] {} filler lines pre-synthesized", FILLERS.len());
}

/// Per-sentence synthesis: sanitize → TTS → WAV → base64 → one `audio` line.
pub(super) struct Synth {
    pub(super) engines: Engines,
    pub(super) out: mpsc::Sender<String>,
    pub(super) idx: u32,
    pub(super) first_audio: Option<Duration>,
    pub(super) t0: Instant,
    pub(super) spoken_sentences: u8,
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

    /// Emit a filler cached for the current live voice profile. The first use
    /// after a voice or speed change synthesizes a fresh variant.
    pub(super) async fn filler(&mut self, index: usize, text: &str) {
        let Some(tts) = self.engines.tts.clone() else {
            return;
        };
        let key = (tts.cache_key(), index);
        let cached = filler_cache().lock().unwrap().get(&key).cloned();
        if let Some(audio) = cached {
            self.cached(&audio).await;
            return;
        }
        let clean = strip_markdown(&strip_spoken(text));
        if clean.is_empty() {
            return;
        }
        let synth_tts = tts.clone();
        let result = tokio::task::spawn_blocking(move || synth_tts.synthesize(&clean))
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
        match result {
            Ok(audio) => {
                let encoded = B64.encode(regent_speech::wav::encode(&audio));
                // The setting may have changed between the miss and inference;
                // key the result by the profile active after synthesis.
                let key = (tts.cache_key(), index);
                filler_cache().lock().unwrap().insert(key, encoded.clone());
                self.cached(&encoded).await;
            }
            Err(e) => {
                self.out
                    .send(json!({"error": format!("TTS: {e}")}).to_string())
                    .await
                    .ok();
            }
        }
    }

    pub(super) async fn sentence(&mut self, text: &str) {
        let mut clean = strip_markdown(&strip_spoken(text));
        if clean.is_empty() {
            return;
        }
        if self.spoken_sentences < MAX_SPOKEN_SENTENCES {
            self.spoken_sentences += 1;
        } else if self.spoken_sentences == MAX_SPOKEN_SENTENCES {
            self.spoken_sentences += 1;
            clean = SPOKEN_HANDOFF.to_owned();
        } else {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::engines::TtsEngine;
    use regent_kernel::AudioBuffer;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct VoiceAwareTts {
        profile: AtomicUsize,
        calls: AtomicUsize,
        spoken: Mutex<Vec<String>>,
    }

    impl TtsEngine for VoiceAwareTts {
        fn synthesize(&self, text: &str) -> Result<AudioBuffer, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.spoken.lock().unwrap().push(text.to_owned());
            Ok(AudioBuffer::new(vec![0, 1, 0], 16_000, 1))
        }

        fn cache_key(&self) -> String {
            format!("test-profile-{}", self.profile.load(Ordering::SeqCst))
        }
    }

    #[tokio::test]
    async fn filler_cache_tracks_live_voice_profile() {
        let tts = Arc::new(VoiceAwareTts {
            profile: AtomicUsize::new(41_001),
            calls: AtomicUsize::new(0),
            spoken: Mutex::new(Vec::new()),
        });
        let engines = Engines {
            tts: Some(tts.clone()),
            ..Engines::default()
        };
        let (out, mut rx) = mpsc::channel(4);
        let mut synth = Synth {
            engines,
            out,
            idx: 0,
            first_audio: None,
            t0: Instant::now(),
            spoken_sentences: 0,
        };

        synth.filler(0, FILLERS[0]).await;
        assert!(rx.recv().await.unwrap().contains("audio"));
        synth.filler(0, FILLERS[0]).await;
        assert!(rx.recv().await.unwrap().contains("audio"));
        assert_eq!(tts.calls.load(Ordering::SeqCst), 1);

        tts.profile.store(41_002, Ordering::SeqCst);
        synth.filler(0, FILLERS[0]).await;
        assert!(rx.recv().await.unwrap().contains("audio"));
        assert_eq!(tts.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn long_reply_speaks_three_sentences_then_one_handoff() {
        let tts = Arc::new(VoiceAwareTts {
            profile: AtomicUsize::new(51_001),
            calls: AtomicUsize::new(0),
            spoken: Mutex::new(Vec::new()),
        });
        let engines = Engines {
            tts: Some(tts.clone()),
            ..Engines::default()
        };
        let (out, mut rx) = mpsc::channel(8);
        let mut synth = Synth {
            engines,
            out,
            idx: 0,
            first_audio: None,
            t0: Instant::now(),
            spoken_sentences: 0,
        };

        for sentence in ["One.", "Two.", "Three.", "Four.", "Five."] {
            synth.sentence(sentence).await;
        }

        assert_eq!(tts.calls.load(Ordering::SeqCst), 4);
        assert_eq!(
            *tts.spoken.lock().unwrap(),
            ["One.", "Two.", "Three.", SPOKEN_HANDOFF]
        );
        for _ in 0..4 {
            assert!(rx.recv().await.unwrap().contains("audio"));
        }
        assert!(rx.try_recv().is_err());
    }
}
