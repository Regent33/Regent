//! Sentence-boundary streaming: the reply arrives token-by-token, and TTS
//! starts on sentence 1 while the model is still writing. Port of the
//! `[.!?…](\s|$)` loop in web_call.py's `/call/turn` emitter.

/// Accumulates streamed text deltas and yields complete sentences as they
/// close. `flush()` returns any trailing partial sentence at end-of-turn.
#[derive(Default)]
pub struct SentenceSplitter {
    pending: String,
}

impl SentenceSplitter {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one delta; returns every sentence completed by it (usually zero
    /// or one, more when a delta carries several boundaries).
    pub fn push(&mut self, delta: &str) -> Vec<String> {
        self.pending.push_str(delta);
        let mut out = Vec::new();
        while let Some(end) = boundary(&self.pending) {
            let rest = self.pending.split_off(end);
            out.push(std::mem::replace(&mut self.pending, rest));
        }
        out
    }

    /// The trailing partial sentence (no closing punctuation), if any.
    pub fn flush(&mut self) -> Option<String> {
        let rest = std::mem::take(&mut self.pending);
        (!rest.trim().is_empty()).then_some(rest)
    }
}

/// Byte index just past the first sentence boundary: a terminator ([.!?…])
/// followed by whitespace (consumed) or end-of-string.
fn boundary(text: &str) -> Option<usize> {
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if !matches!(c, '.' | '!' | '?' | '…') {
            continue;
        }
        match chars.peek() {
            Some((j, next)) if next.is_whitespace() => return Some(j + next.len_utf8()),
            None => return Some(i + c.len_utf8()),
            _ => {} // mid-token punctuation ("3.14", "e.g.x") — keep scanning
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentences_stream_out_as_they_close() {
        let mut s = SentenceSplitter::new();
        assert!(s.push("Hello wor").is_empty(), "no boundary yet");
        assert_eq!(s.push("ld. How are").as_slice(), ["Hello world. "]);
        assert_eq!(s.push(" you? I'm fine").as_slice(), ["How are you? "]);
        assert_eq!(s.flush().as_deref(), Some("I'm fine"));
        assert_eq!(s.flush(), None, "flush drains");
    }

    #[test]
    fn one_delta_can_close_many_sentences() {
        let mut s = SentenceSplitter::new();
        let out = s.push("One. Two! Three… tail");
        assert_eq!(out.as_slice(), ["One. ", "Two! ", "Three… "]);
        assert_eq!(s.flush().as_deref(), Some("tail"));
    }

    #[test]
    fn decimal_points_do_not_split() {
        let mut s = SentenceSplitter::new();
        assert!(s.push("pi is 3.14159 about").is_empty());
        assert_eq!(s.push(".").as_slice(), ["pi is 3.14159 about."]);
    }
}
