//! local — a turn-based call brain on **local models** (the Qwen3 speech server),
//! the offline alternative to [`crate::openai_realtime`]. No API key, no cloud.
//!
//! A local call can't be true speech-to-speech, so it's turn-based: detect when
//! the caller stops talking (VAD), transcribe that utterance (local ASR), answer
//! it (the agent), speak the reply (local TTS), and emit it as the engine's
//! [`ProviderEvent::Audio`]. ASR/TTS and the agent are **injected** (the HTTP
//! calls to localhost:8000 + the agent live at the composition root, where
//! reqwest already is), so this module is pure and testable offline.

use crate::{AudioFrame, ProviderEvent};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Local speech I/O — the Qwen3 server's `/v1/audio/*`. Injected so the provider
/// is testable without HTTP; production POSTs to the local server.
#[async_trait]
pub trait SpeechIo: Send + Sync {
    async fn transcribe(&self, pcm: &[i16], sample_rate: u32) -> String;
    async fn synthesize(&self, text: &str) -> AudioFrame;
}

/// The conversational brain: caller text → reply text. Wraps Regent's agent in
/// production, a stub in tests. (Tool calls can ride a richer return type later.)
#[async_trait]
pub trait Brain: Send + Sync {
    async fn respond(&self, text: &str) -> String;
}

/// Energy-gated turn detector: feed it caller frames; it hands back the buffered
/// utterance once the caller goes quiet after speaking.
///
/// ponytail: RMS energy + a trailing-silence counter — zero deps. Swap for
/// webrtc-vad/Silero if it false-triggers on background noise.
pub struct TurnDetector {
    threshold: f64,   // RMS above this counts as speech
    hang_frames: u32, // this many consecutive quiet frames after speech ends a turn
    buf: Vec<i16>,
    quiet: u32,
    in_speech: bool,
}

impl TurnDetector {
    pub fn new(threshold: f64, hang_frames: u32) -> Self {
        Self {
            threshold,
            hang_frames,
            buf: Vec::new(),
            quiet: 0,
            in_speech: false,
        }
    }

    fn rms(pcm: &[i16]) -> f64 {
        if pcm.is_empty() {
            return 0.0;
        }
        let sum: f64 = pcm.iter().map(|&s| (s as f64) * (s as f64)).sum();
        (sum / pcm.len() as f64).sqrt()
    }

    /// Returns `Some(utterance_pcm)` when a turn just ended, else `None`.
    pub fn push(&mut self, frame: &AudioFrame) -> Option<Vec<i16>> {
        if Self::rms(&frame.pcm) >= self.threshold {
            self.in_speech = true;
            self.quiet = 0;
            self.buf.extend_from_slice(&frame.pcm);
            return None;
        }
        if !self.in_speech {
            return None; // silence before any speech — ignore
        }
        self.buf.extend_from_slice(&frame.pcm); // keep the trailing tail
        self.quiet += 1;
        if self.quiet >= self.hang_frames {
            self.quiet = 0;
            self.in_speech = false;
            return Some(std::mem::take(&mut self.buf));
        }
        None
    }
}

/// Drive the local turn loop: caller audio → VAD → ASR → brain → TTS → audio out.
/// Runs as the engine's "provider" task (it owns the caller-audio receiver and the
/// event sender [`crate::ProviderEnds`] uses). Ends when the caller channel closes.
pub async fn run_local_provider(
    mut audio_in: mpsc::Receiver<AudioFrame>,
    events: mpsc::Sender<ProviderEvent>,
    speech: Arc<dyn SpeechIo>,
    brain: Arc<dyn Brain>,
    mut detector: TurnDetector,
) {
    while let Some(frame) = audio_in.recv().await {
        let Some(utterance) = detector.push(&frame) else {
            continue;
        };
        let text = speech.transcribe(&utterance, frame.sample_rate).await;
        if text.trim().is_empty() {
            continue; // VAD blip / non-speech
        }
        let reply = brain.respond(&text).await;
        if events
            .send(ProviderEvent::Audio(speech.synthesize(&reply).await))
            .await
            .is_err()
        {
            break; // caller gone
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(amp: i16) -> AudioFrame {
        AudioFrame {
            pcm: vec![amp; 4],
            sample_rate: 24_000,
        }
    }

    #[test]
    fn detector_segments_on_trailing_silence() {
        let mut d = TurnDetector::new(100.0, 2);
        assert_eq!(d.push(&frame(0)), None); // silence before speech → ignored
        assert_eq!(d.push(&frame(5000)), None); // speech
        assert_eq!(d.push(&frame(0)), None); // quiet 1
        let utt = d.push(&frame(0)).expect("turn ends after 2 quiet frames"); // quiet 2
        assert_eq!(utt.len(), 12); // 1 speech + 2 trailing-silence frames × 4 samples
        // a new turn starts clean
        assert_eq!(d.push(&frame(5000)), None);
    }

    struct StubSpeech;
    #[async_trait]
    impl SpeechIo for StubSpeech {
        async fn transcribe(&self, pcm: &[i16], _sr: u32) -> String {
            if pcm.is_empty() {
                String::new()
            } else {
                "hello".into()
            }
        }
        async fn synthesize(&self, text: &str) -> AudioFrame {
            AudioFrame {
                pcm: vec![text.len() as i16],
                sample_rate: 24_000,
            }
        }
    }
    struct EchoBrain;
    #[async_trait]
    impl Brain for EchoBrain {
        async fn respond(&self, text: &str) -> String {
            format!("you said {text}")
        }
    }

    #[tokio::test]
    async fn a_full_turn_transcribes_answers_and_speaks() {
        let (tx, rx) = mpsc::channel(8);
        let (ev_tx, mut ev_rx) = mpsc::channel(8);
        let task = tokio::spawn(run_local_provider(
            rx,
            ev_tx,
            Arc::new(StubSpeech),
            Arc::new(EchoBrain),
            TurnDetector::new(100.0, 1),
        ));
        tx.send(frame(5000)).await.unwrap(); // speech
        tx.send(frame(0)).await.unwrap(); // silence → turn ends → ASR→brain→TTS
        // "you said hello" is 14 chars → the stub TTS encodes that length
        match ev_rx.recv().await.unwrap() {
            ProviderEvent::Audio(a) => assert_eq!(a.pcm, vec![14]),
            other => panic!("expected audio, got {other:?}"),
        }
        drop(tx);
        task.await.unwrap();
    }
}
