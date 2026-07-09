//! Server-side voice-activity gate — the safety net behind the client's VAD.
//!
//! The call UI (desktop `callLoop.ts`, web `localCall.ts`) already energy-gates
//! the mic before POSTing, but its VAD lives on the browser main thread and
//! degrades under load, and a noise burst can still clip past its threshold.
//! When that noisy blip reaches `/call/turn`, **whisper hallucinates words from
//! room noise** ("thank you", "thanks for watching", "you") — and that phantom
//! text then drives a full agent turn, which is the reported "picks up noise and
//! breaks the conversation" bug. It also wastes latency running whisper on
//! nothing.
//!
//! So we gate here too, in two cheap stages over the decoded PCM:
//!   1. **Pre-ASR** — reject near-silence (peak RMS below a floor) or too-short
//!      blips (voiced duration under a minimum) *before* whisper runs.
//!   2. **Post-ASR** — if the audio was quiet AND the transcript is one of
//!      whisper's stock silence-hallucinations, drop it.
//!
//! Every threshold is env-tunable (`REGENT_VAD_*`) because mic gain and room
//! noise are physical and need calibration. Defaults are deliberately *more
//! permissive* than the client so this never rejects speech the client already
//! accepted — it only catches the degenerate noise/silence cases.

/// ~20 ms analysis frames (independent of the client's 4096-sample capture).
const FRAME_MS: f32 = 20.0;

/// Tunable gate thresholds, read once per turn from `REGENT_VAD_*`.
#[derive(Clone, Copy, Debug)]
pub struct VadConfig {
    /// A frame counts as "voiced" (and the clip as non-silent) only if its RMS
    /// exceeds this. `REGENT_VAD_MIN_RMS` (default 0.010). Below the client's
    /// 0.015 onset so we never reject what the client let through.
    pub min_rms: f32,
    /// Minimum voiced duration for a real utterance. Shorter → a click/pop, not
    /// speech. `REGENT_VAD_MIN_SPEECH_MS` (default 120 ms).
    pub min_speech_secs: f32,
    /// A transcript matching a known whisper silence-hallucination is dropped
    /// only when the voiced RMS is below this (i.e. the audio was too quiet to
    /// really carry those words). `REGENT_VAD_HALLUCINATION_RMS` (default
    /// 0.020). Set to 0 to disable the phrase filter entirely.
    pub hallucination_rms: f32,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            min_rms: 0.010,
            min_speech_secs: 0.120,
            hallucination_rms: 0.020,
        }
    }
}

impl VadConfig {
    /// Read the knobs from the environment, falling back to [`Default`].
    #[must_use]
    pub fn from_env() -> Self {
        let d = Self::default();
        Self {
            min_rms: env_f32("REGENT_VAD_MIN_RMS", d.min_rms),
            min_speech_secs: env_f32("REGENT_VAD_MIN_SPEECH_MS", d.min_speech_secs * 1000.0) / 1000.0,
            hallucination_rms: env_f32("REGENT_VAD_HALLUCINATION_RMS", d.hallucination_rms),
        }
    }
}

fn env_f32(key: &str, default: f32) -> f32 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<f32>().ok())
        .filter(|v| v.is_finite() && *v >= 0.0)
        .unwrap_or(default)
}

/// Energy summary of a decoded utterance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioStats {
    /// Loudest 20 ms frame's RMS — the clip is silent if this is tiny.
    pub peak_rms: f32,
    /// Seconds of audio whose frame RMS exceeded the voiced floor.
    pub voiced_secs: f32,
    /// Mean RMS across the voiced frames (0 if none) — how loud the speech was.
    pub voiced_rms: f32,
}

/// Frame the signal and summarize its energy. `voiced_floor` is the per-frame
/// RMS above which a frame is "voiced" (pass [`VadConfig::min_rms`]).
#[must_use]
pub fn analyze(samples: &[f32], rate: u32, voiced_floor: f32) -> AudioStats {
    let frame = ((rate as f32 * FRAME_MS / 1000.0) as usize).max(1);
    let frame_secs = frame as f32 / rate.max(1) as f32;
    let mut peak_rms = 0.0f32;
    let mut voiced_frames = 0u32;
    let mut voiced_sum = 0.0f32;
    for chunk in samples.chunks(frame) {
        let sum: f32 = chunk.iter().map(|s| s * s).sum();
        let rms = (sum / chunk.len().max(1) as f32).sqrt();
        peak_rms = peak_rms.max(rms);
        if rms > voiced_floor {
            voiced_frames += 1;
            voiced_sum += rms;
        }
    }
    AudioStats {
        peak_rms,
        voiced_secs: voiced_frames as f32 * frame_secs,
        voiced_rms: if voiced_frames == 0 {
            0.0
        } else {
            voiced_sum / voiced_frames as f32
        },
    }
}

/// Pre-ASR decision: `None` = let it through, `Some(reason)` = drop as noise.
#[must_use]
pub fn pre_asr_reject(stats: &AudioStats, cfg: &VadConfig) -> Option<&'static str> {
    if stats.peak_rms < cfg.min_rms {
        return Some("below noise floor");
    }
    if stats.voiced_secs < cfg.min_speech_secs {
        return Some("too short");
    }
    None
}

/// Whisper's stock outputs on silence/room-noise. Compared case- and
/// trailing-punctuation-insensitively. Kept short and unambiguous — none is a
/// plausible *quiet* real turn once the energy gate below also applies.
const HALLUCINATIONS: &[&str] = &[
    "you",
    "thank you",
    "thank you.",
    "thanks for watching",
    "thanks for watching!",
    "thank you for watching",
    "thank you very much",
    "please subscribe",
    "subscribe",
    "bye",
    "bye-bye",
    "so",
    ".",
    "。",
    "the",
];

/// Normalize a transcript for hallucination matching: trim, lowercase, drop
/// surrounding whitespace/quotes (keep inner text).
fn normalize(text: &str) -> String {
    text.trim()
        .trim_matches(|c: char| c == '"' || c == '\'' || c.is_whitespace())
        .to_lowercase()
}

/// True when `text` is a likely whisper hallucination from quiet audio: it
/// matches a stock phrase AND the audio was below [`VadConfig::hallucination_rms`]
/// (so genuinely-spoken words, which are louder, are never dropped). Disabled
/// when `hallucination_rms` is 0.
#[must_use]
pub fn is_noise_hallucination(text: &str, stats: &AudioStats, cfg: &VadConfig) -> bool {
    if cfg.hallucination_rms <= 0.0 || stats.voiced_rms >= cfg.hallucination_rms {
        return false;
    }
    let norm = normalize(text);
    HALLUCINATIONS.iter().any(|h| normalize(h) == norm)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, secs: f32, amp: f32) -> Vec<f32> {
        let n = (rate as f32 * secs) as usize;
        (0..n)
            .map(|i| amp * (i as f32 * 0.1).sin())
            .collect()
    }

    #[test]
    fn silence_is_rejected_before_asr() {
        let cfg = VadConfig::default();
        let s = analyze(&tone(16_000, 1.0, 0.001), 16_000, cfg.min_rms);
        assert_eq!(pre_asr_reject(&s, &cfg), Some("below noise floor"));
    }

    #[test]
    fn a_short_click_is_rejected() {
        let cfg = VadConfig::default();
        // 50 ms of loud audio — above the floor but under min_speech (120 ms).
        let s = analyze(&tone(16_000, 0.05, 0.3), 16_000, cfg.min_rms);
        assert!(s.peak_rms > cfg.min_rms);
        assert_eq!(pre_asr_reject(&s, &cfg), Some("too short"));
    }

    #[test]
    fn real_speech_passes() {
        let cfg = VadConfig::default();
        let s = analyze(&tone(16_000, 0.8, 0.2), 16_000, cfg.min_rms);
        assert_eq!(pre_asr_reject(&s, &cfg), None);
    }

    #[test]
    fn quiet_thank_you_is_dropped_but_loud_one_survives() {
        let cfg = VadConfig::default();
        let quiet = AudioStats {
            peak_rms: 0.015,
            voiced_secs: 0.4,
            voiced_rms: 0.012,
        };
        assert!(is_noise_hallucination("Thank you.", &quiet, &cfg));
        assert!(is_noise_hallucination(" you ", &quiet, &cfg));
        let loud = AudioStats {
            voiced_rms: 0.05,
            ..quiet
        };
        assert!(
            !is_noise_hallucination("Thank you.", &loud, &cfg),
            "a clearly-spoken thank-you must never be dropped"
        );
    }

    #[test]
    fn normal_reply_is_never_a_hallucination() {
        let cfg = VadConfig::default();
        let quiet = AudioStats {
            peak_rms: 0.015,
            voiced_secs: 0.4,
            voiced_rms: 0.012,
        };
        assert!(!is_noise_hallucination("what's on my calendar today", &quiet, &cfg));
    }

    #[test]
    fn hallucination_filter_disables_at_zero() {
        let cfg = VadConfig {
            hallucination_rms: 0.0,
            ..VadConfig::default()
        };
        let quiet = AudioStats {
            peak_rms: 0.015,
            voiced_secs: 0.4,
            voiced_rms: 0.012,
        };
        assert!(!is_noise_hallucination("thank you", &quiet, &cfg));
    }

    #[test]
    fn env_parsing_survives_garbage() {
        // Unset → defaults; the parse just needs to not panic on bad input.
        let d = VadConfig::default();
        assert!((env_f32("REGENT_VAD_DEFINITELY_UNSET_XYZ", d.min_rms) - d.min_rms).abs() < 1e-9);
    }
}
