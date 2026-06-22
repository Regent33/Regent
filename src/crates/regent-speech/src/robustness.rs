//! ASR robustness — the warts Hermes learned the hard way, ported as pure
//! functions: a hallucination filter (Whisper invents "thank you" /
//! "subscribe" on silence) and oversized-file chunk planning (provider APIs cap
//! upload size). Both are testable without any model or network.

/// Phrases Whisper commonly hallucinates on silent/near-silent audio. Matched
/// case-insensitively after stripping trailing `.`/`!`. Ported from
/// `voice_mode.py::WHISPER_HALLUCINATIONS`.
const HALLUCINATIONS: &[&str] = &[
    "thank you",
    "thanks for watching",
    "thank you for watching",
    "subscribe to my channel",
    "like and subscribe",
    "please subscribe",
    "bye",
    "you",
    "the end",
    "продолжение следует",
    "sous-titres",
    "amara.org",
    "ご視聴ありがとうございました",
];

/// Filler tokens that, when a transcript is made up *entirely* of them,
/// indicate a repetitive hallucination (e.g. "Thank you. Thank you. you").
const FILLER_WORDS: &[&str] = &["thank", "you", "thanks", "bye", "ok", "okay", "the", "end"];

/// True when `text` is empty or a known silence hallucination — callers treat
/// such a transcript as "nothing was said" (return `""`, don't feed the agent).
#[must_use]
pub fn is_hallucination(text: &str) -> bool {
    let cleaned = text.trim().to_lowercase();
    if cleaned.is_empty() {
        return true;
    }
    let stripped = cleaned.trim_end_matches(['.', '!', ' ']);
    if HALLUCINATIONS.contains(&stripped) || HALLUCINATIONS.contains(&cleaned.as_str()) {
        return true;
    }
    // All-filler (after dropping punctuation) ⇒ repetitive hallucination.
    let words: Vec<&str> = cleaned
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    !words.is_empty() && words.iter().all(|w| FILLER_WORDS.contains(w))
}

/// Drop a hallucination to an empty string, else return the trimmed transcript.
#[must_use]
pub fn clean_transcript(text: &str) -> String {
    if is_hallucination(text) {
        String::new()
    } else {
        text.trim().to_string()
    }
}

/// Plan byte ranges for splitting `data_len` bytes into chunks no larger than
/// `max_chunk` and aligned to `block_align` (so PCM frames aren't split mid-
/// sample). Returns `(offset, len)` pairs covering `0..data_len`. Empty input
/// yields no chunks. The real WAV chunker (V0.4 ffmpeg path) uses this to size
/// chunks under a provider's upload cap; this is the pure arithmetic.
#[must_use]
pub fn chunk_ranges(data_len: usize, max_chunk: usize, block_align: usize) -> Vec<(usize, usize)> {
    let align = block_align.max(1);
    // Largest block-aligned size that still fits the cap (at least one block).
    let step = ((max_chunk / align).max(1)) * align;
    let mut ranges = Vec::new();
    let mut offset = 0;
    while offset < data_len {
        let len = step.min(data_len - offset);
        ranges.push((offset, len));
        offset += len;
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_known_and_repetitive_hallucinations() {
        assert!(is_hallucination("Thank you."));
        assert!(is_hallucination("thanks for watching"));
        assert!(is_hallucination("Thank you. Thank you. you"));
        assert!(is_hallucination("   "));
        assert!(is_hallucination(""));
    }

    #[test]
    fn keeps_real_speech() {
        assert!(!is_hallucination("what's the weather tomorrow"));
        assert_eq!(clean_transcript("  hello there  "), "hello there");
        assert_eq!(clean_transcript("thank you."), "");
    }

    #[test]
    fn chunk_ranges_are_aligned_and_cover_everything() {
        // 100 bytes, cap 30, align 4 → step 28; chunks 0..28,28..56,56..84,84..100.
        let ranges = chunk_ranges(100, 30, 4);
        assert_eq!(ranges, vec![(0, 28), (28, 28), (56, 28), (84, 16)]);
        for (_, len) in &ranges {
            assert!(*len <= 30);
        }
        // Reassembles to the whole.
        let covered: usize = ranges.iter().map(|(_, l)| l).sum();
        assert_eq!(covered, 100);
    }

    #[test]
    fn chunk_ranges_handles_empty_and_small() {
        assert!(chunk_ranges(0, 100, 2).is_empty());
        assert_eq!(chunk_ranges(10, 100, 2), vec![(0, 10)]);
    }
}
