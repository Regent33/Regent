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
/// After the first spoken filler, speak another roughly this often while the
/// silence continues (tool runs, deep searches) — one line per turn left the
/// caller in dead air for minutes.
pub(super) const FILLER_REPEAT: Duration = Duration::from_secs(16);
/// Cap the spoken bridge lines per silence gap; past this, keepalives only —
/// a fifth "still working" reads as a broken record, not reassurance.
pub(super) const MAX_FILLERS_PER_GAP: usize = 4;
/// Give up on a turn only after this much *continuous* brain silence (a real
/// stall — deacon hung or dropped), rather than keepalive-ing forever. 10 min:
/// deep context searches legitimately stream nothing for minutes (keepalives
/// bridge the client), and 3 min was ending real turns early.
pub(super) const STALL_TIMEOUT: Duration = Duration::from_secs(600);
/// How many sentences Butler actually SPEAKS before handing off to the screen.
/// The full reply always streams to the transcript; this caps only the voice.
/// A low cap cut explanations off mid-thought (text complete, but the voice
/// stopped), so this is set high enough to speak a real, long explanation all
/// the way through — it is only a backstop against a runaway essay, never the
/// target length. The conversational prompt keeps ordinary chatter short.
const MAX_SPOKEN_SENTENCES: u8 = 12;
const SPOKEN_HANDOFF: &str = "I've put the rest on screen.";
pub(super) const FILLERS: [&str; 12] = [
    "Just a sec.",
    "One moment.",
    "On it.",
    "Let me check.",
    "Hmm, let me see.",
    "Give me a second.",
    "Looking now.",
    "Okay, hold on.",
    "Still on it.",
    "Checking a few things.",
    "Almost there.",
    "Bear with me.",
];

/// Whether to speak one bridging line because the CALLER is hearing dead air.
///
/// Brain silence and audio silence are not the same thing: the model can stream
/// tokens steadily while the first sentence is still being assembled and
/// synthesized. Measured on 2026-07-25, first token landed at 2.1s and first
/// audio at 6.27s — four seconds of silence that the silence-gap fillers never
/// covered, because from their point of view the brain was busy talking. Keying
/// on `first_audio` bridges exactly that gap, at no synthesis cost when the
/// filler cache is warm. `bridged` is required because a missing TTS engine
/// leaves `first_audio` unset, which would otherwise re-fire on every delta.
pub(super) fn should_bridge_dead_air(
    first_audio: Option<Duration>,
    elapsed: Duration,
    bridged: bool,
) -> bool {
    first_audio.is_none() && !bridged && elapsed >= FILLER_WAIT
}

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
        // The client vetoes a barge by matching what it heard against what
        // Regent is saying — and a filler is the ONE thing he says that never
        // arrived as a `reply` line. It plays precisely when no reply text
        // exists yet (that is why it plays at all), so the veto had nothing to
        // compare against and every filler echo was promoted as a real
        // interruption, killing the turn before the answer began.
        // Additive: an older client ignores the field.
        self.out
            .send(json!({"filler": text}).to_string())
            .await
            .ok();
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

    // Live 2026-07-25 turns: brain_ttft 1.26–2.1s but first_audio 3.62–7.04s.
    // The brain was streaming the whole time, so the silence-gap fillers never
    // fired and the caller heard 2–4s of dead air after Regent had "started
    // talking". The bridge must key on first AUDIO, not on the first token.
    #[test]
    fn dead_air_is_bridged_while_tokens_stream_but_nothing_has_been_spoken() {
        // 2.1s in, tokens flowing, no audio yet → not due yet.
        assert!(!should_bridge_dead_air(
            None,
            Duration::from_millis(2100),
            false
        ));
        // Past the wait with still no audio → speak one cached line.
        assert!(should_bridge_dead_air(
            None,
            Duration::from_millis(2600),
            false
        ));
        // Already bridged, and TTS never set first_audio → never loop on it.
        assert!(!should_bridge_dead_air(None, Duration::from_secs(9), true));
        // Real audio is playing → the bridge is irrelevant.
        assert!(!should_bridge_dead_air(
            Some(Duration::from_millis(3620)),
            Duration::from_secs(9),
            false
        ));
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

        // Each filler is now TWO lines: its text, then its audio. The text is
        // what lets the client's echo veto recognize these words as Regent's
        // own — a filler plays while no reply text exists, so without it the
        // veto was blind exactly when the filler could echo.
        let mut speak_filler = async |synth: &mut Synth| {
            synth.filler(0, FILLERS[0]).await;
            let text = rx.recv().await.unwrap();
            assert!(text.contains("filler"), "filler text first: {text}");
            assert!(text.contains(FILLERS[0]), "carries the words: {text}");
            assert!(rx.recv().await.unwrap().contains("audio"));
        };

        speak_filler(&mut synth).await;
        speak_filler(&mut synth).await;
        assert_eq!(tts.calls.load(Ordering::SeqCst), 1, "second use is cached");

        tts.profile.store(41_002, Ordering::SeqCst);
        speak_filler(&mut synth).await;
        assert_eq!(
            tts.calls.load(Ordering::SeqCst),
            2,
            "a voice change re-synthesizes"
        );
    }

    // The cap is a runaway-essay backstop (raised 3 → 12 in 7c42efd, "speak
    // long explanations in full"); this asserted the pre-raise 3-sentence
    // behavior and had been failing against the shipped contract ever since.
    #[tokio::test]
    async fn runaway_reply_speaks_the_cap_then_one_handoff() {
        let tts = Arc::new(VoiceAwareTts {
            profile: AtomicUsize::new(51_001),
            calls: AtomicUsize::new(0),
            spoken: Mutex::new(Vec::new()),
        });
        let engines = Engines {
            tts: Some(tts.clone()),
            ..Engines::default()
        };
        let (out, mut rx) = mpsc::channel(16);
        let mut synth = Synth {
            engines,
            out,
            idx: 0,
            first_audio: None,
            t0: Instant::now(),
            spoken_sentences: 0,
        };

        let cap = usize::from(MAX_SPOKEN_SENTENCES);
        let sentences: Vec<String> = (1..=cap + 2).map(|i| format!("Sentence {i}.")).collect();
        for sentence in &sentences {
            synth.sentence(sentence).await;
        }

        // Every sentence up to the cap speaks, then exactly one handoff line;
        // anything past that is text-only (the transcript still carries it).
        assert_eq!(tts.calls.load(Ordering::SeqCst), cap + 1);
        // Cloned, not borrowed: `await_holding_lock` does not follow the
        // explicit `drop` this used to rely on, so `-D warnings` failed on a
        // guard that was already released. Copying six strings is cheaper than
        // the workaround.
        let spoken = tts.spoken.lock().unwrap().clone();
        assert_eq!(spoken.len(), cap + 1);
        assert_eq!(spoken[..cap], sentences[..cap]);
        assert_eq!(spoken[cap], SPOKEN_HANDOFF);
        for _ in 0..=cap {
            assert!(rx.recv().await.unwrap().contains("audio"));
        }
        assert!(rx.try_recv().is_err());
    }
}
