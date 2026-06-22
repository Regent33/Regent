//! Voice-activity detection — energy-based turn segmentation, ported from
//! Hermes `voice_mode.py::AudioRecorder`'s silence detector. Frame-driven and
//! pure (no audio device), so it's unit-testable on fixture PCM and reusable
//! for both push-to-talk auto-stop (V1b) and real-time barge-in (V4).
//!
//! ponytail: this is the core detector (confirm speech after `min_speech_ms`,
//! stop after `silence_ms` of trailing silence, give up after `max_wait_ms`
//! with no speech). The micro-dip tolerance hysteresis from the Python version
//! is a refinement to add if syllable gaps prove to cut turns short.

/// Tunables for [`Vad`]. Defaults mirror `voice_mode.py`.
#[derive(Debug, Clone, Copy)]
pub struct VadConfig {
    pub sample_rate: u32,
    /// RMS at or below this is silence (int16 range, default 200).
    pub silence_rms: i32,
    /// Trailing silence after speech that ends the turn (default 3000 ms).
    pub silence_ms: u32,
    /// Continuous speech needed to confirm a real utterance (default 300 ms).
    pub min_speech_ms: u32,
    /// Give up if no speech is heard at all within this window (default 15000 ms).
    pub max_wait_ms: u32,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16_000,
            silence_rms: 200,
            silence_ms: 3_000,
            min_speech_ms: 300,
            max_wait_ms: 15_000,
        }
    }
}

/// A streaming silence detector. Feed it mono `i16` frames; [`Vad::push`]
/// returns `true` once capture should stop.
#[derive(Debug)]
pub struct Vad {
    cfg: VadConfig,
    has_spoken: bool,
    speech_ms: u32,
    silence_ms: u32,
    elapsed_ms: u32,
    peak_rms: i32,
}

impl Vad {
    #[must_use]
    pub fn new(cfg: VadConfig) -> Self {
        Self {
            cfg,
            has_spoken: false,
            speech_ms: 0,
            silence_ms: 0,
            elapsed_ms: 0,
            peak_rms: 0,
        }
    }

    /// Feed one mono frame. Returns `true` when capture should stop:
    /// speech was heard then `silence_ms` of quiet followed, or no speech
    /// arrived within `max_wait_ms`.
    pub fn push(&mut self, frame: &[i16]) -> bool {
        let frame_ms = frame_duration_ms(frame.len(), self.cfg.sample_rate);
        self.elapsed_ms = self.elapsed_ms.saturating_add(frame_ms);
        let level = rms(frame);
        self.peak_rms = self.peak_rms.max(level);

        if level > self.cfg.silence_rms {
            self.speech_ms = self.speech_ms.saturating_add(frame_ms);
            self.silence_ms = 0;
            if self.speech_ms >= self.cfg.min_speech_ms {
                self.has_spoken = true;
            }
        } else if self.has_spoken {
            self.silence_ms = self.silence_ms.saturating_add(frame_ms);
            if self.silence_ms >= self.cfg.silence_ms {
                return true; // spoke, then went quiet → end of turn
            }
        } else {
            // Below-threshold spikes before any confirmed speech don't count.
            self.speech_ms = 0;
        }

        // No speech at all within the patience window → give up.
        !self.has_spoken && self.elapsed_ms >= self.cfg.max_wait_ms
    }

    /// Loudest frame seen so far — used to discard a too-quiet capture.
    #[must_use]
    pub fn peak_rms(&self) -> i32 {
        self.peak_rms
    }

    /// Whether a real utterance was confirmed (vs. silence/noise only).
    #[must_use]
    pub fn had_speech(&self) -> bool {
        self.has_spoken
    }
}

/// Root-mean-square amplitude of a frame (0 for an empty frame).
#[must_use]
pub fn rms(frame: &[i16]) -> i32 {
    if frame.is_empty() {
        return 0;
    }
    let sum_sq: i64 = frame.iter().map(|&s| i64::from(s) * i64::from(s)).sum();
    ((sum_sq / frame.len() as i64) as f64).sqrt() as i32
}

/// Duration of a mono frame in ms (0 if `sample_rate` is 0).
fn frame_duration_ms(samples: usize, sample_rate: u32) -> u32 {
    if sample_rate == 0 {
        return 0;
    }
    ((samples as u64 * 1000) / u64::from(sample_rate)) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 16_000;
    // 100 ms mono frame = 1600 samples.
    const FRAME: usize = 1_600;

    fn loud() -> Vec<i16> {
        vec![5_000; FRAME]
    }
    fn quiet() -> Vec<i16> {
        vec![0; FRAME]
    }

    fn vad() -> Vad {
        Vad::new(VadConfig {
            sample_rate: RATE,
            silence_rms: 200,
            silence_ms: 500,
            min_speech_ms: 300,
            max_wait_ms: 2_000,
        })
    }

    #[test]
    fn rms_of_constant_amplitude() {
        assert_eq!(rms(&[1_000; 100]), 1_000);
        assert_eq!(rms(&[]), 0);
    }

    #[test]
    fn stops_after_speech_then_trailing_silence() {
        let mut v = vad();
        // 400 ms speech (>300 confirm), no stop yet.
        for _ in 0..4 {
            assert!(!v.push(&loud()));
        }
        assert!(v.had_speech());
        // Trailing silence: 500 ms = 5 quiet frames → stop on the 5th.
        assert!(!v.push(&quiet())); // 100
        assert!(!v.push(&quiet())); // 200
        assert!(!v.push(&quiet())); // 300
        assert!(!v.push(&quiet())); // 400
        assert!(v.push(&quiet())); // 500 → stop
        assert!(v.peak_rms() > 200);
    }

    #[test]
    fn gives_up_when_no_speech_within_max_wait() {
        let mut v = vad();
        // 2000 ms patience = 20 quiet frames; stop on the 20th, never spoke.
        let mut stopped = false;
        for _ in 0..20 {
            stopped = v.push(&quiet());
        }
        assert!(stopped);
        assert!(!v.had_speech());
    }

    #[test]
    fn brief_noise_before_speech_does_not_confirm() {
        let mut v = vad();
        // Two loud frames (200 ms < 300 min) then quiet — not confirmed speech,
        // so it should fall through to the max-wait path, not the silence path.
        assert!(!v.push(&loud()));
        assert!(!v.push(&loud()));
        assert!(!v.had_speech());
    }
}
