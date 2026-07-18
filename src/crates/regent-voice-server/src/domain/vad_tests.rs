//! Unit tests for `vad` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

fn tone(rate: u32, secs: f32, amp: f32) -> Vec<f32> {
    let n = (rate as f32 * secs) as usize;
    (0..n).map(|i| amp * (i as f32 * 0.1).sin()).collect()
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
    let mut speech = tone(16_000, 0.6, 0.2);
    speech.splice(0..0, vec![0.002; 3_200]); // quiet pre-roll establishes the room floor
    speech.extend(vec![0.002; 3_200]); // and the normal trailing pause
    let s = analyze(&speech, 16_000, cfg.min_rms);
    assert_eq!(pre_asr_reject(&s, &cfg), None);
}

#[test]
fn quiet_speech_admitted_by_the_client_is_not_rejected_by_the_server() {
    let cfg = VadConfig::default();
    // This tone has ~0.0085 RMS: above the client's quiet-room 0.006 onset,
    // but below the server's old fixed 0.010 floor.
    let mut speech = tone(16_000, 0.6, 0.012);
    speech.splice(0..0, vec![0.001; 3_200]);
    speech.extend(vec![0.001; 3_200]);
    let stats = analyze(&speech, 16_000, cfg.min_rms);
    assert_eq!(pre_asr_reject(&stats, &cfg), None);
}

#[test]
fn speech_just_above_a_loud_room_floor_remains_valid() {
    let cfg = VadConfig::default();
    let stats = AudioStats {
        peak_rms: 0.026,
        voiced_secs: 0.8,
        voiced_rms: 0.024,
        floor_rms: 0.021,
    };
    assert_eq!(pre_asr_reject(&stats, &cfg), None);
}

#[test]
fn continuous_background_is_rejected_even_when_loud() {
    let cfg = VadConfig::default();
    let s = analyze(&tone(16_000, 0.8, 0.05), 16_000, cfg.min_rms);
    assert!(s.peak_rms > cfg.min_rms);
    assert_eq!(
        pre_asr_reject(&s, &cfg),
        Some("stationary background noise")
    );
}

#[test]
fn quiet_thank_you_is_dropped_but_loud_one_survives() {
    let cfg = VadConfig::default();
    let quiet = AudioStats {
        peak_rms: 0.015,
        voiced_secs: 0.4,
        voiced_rms: 0.012,
        floor_rms: 0.005,
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
fn acoustic_annotations_are_dropped_even_when_loud() {
    let cfg = VadConfig::default();
    let loud = AudioStats {
        peak_rms: 0.2,
        voiced_secs: 0.8,
        voiced_rms: 0.15,
        floor_rms: 0.02,
    };
    for transcript in [
        "[BLANK_AUDIO]",
        "[BOOM]",
        "[static]",
        "(static)",
        "[MUSIC PLAYING]",
        "(background music continues)",
    ] {
        assert!(
            is_noise_hallucination(transcript, &loud, &cfg),
            "{transcript} must not become a user turn"
        );
    }
}

#[test]
fn unwrapped_acoustic_words_are_preserved_as_possible_speech() {
    let cfg = VadConfig::default();
    let loud = AudioStats {
        peak_rms: 0.2,
        voiced_secs: 0.8,
        voiced_rms: 0.15,
        floor_rms: 0.02,
    };
    assert!(!is_noise_hallucination("static", &loud, &cfg));
    assert!(!is_noise_hallucination("boom", &loud, &cfg));
}

#[test]
fn normal_reply_is_never_a_hallucination() {
    let cfg = VadConfig::default();
    let quiet = AudioStats {
        peak_rms: 0.015,
        voiced_secs: 0.4,
        voiced_rms: 0.012,
        floor_rms: 0.005,
    };
    assert!(!is_noise_hallucination(
        "what's on my calendar today",
        &quiet,
        &cfg
    ));
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
        floor_rms: 0.005,
    };
    assert!(!is_noise_hallucination("thank you", &quiet, &cfg));
}

#[test]
fn env_parsing_survives_garbage() {
    // Unset → defaults; the parse just needs to not panic on bad input.
    let d = VadConfig::default();
    assert!((env_f32("REGENT_VAD_DEFINITELY_UNSET_XYZ", d.min_rms) - d.min_rms).abs() < 1e-9);
}
