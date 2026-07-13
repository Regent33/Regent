//! Keeps fenced code out of the *spoken* stream while the full reply still
//! reaches the client. The model may append a ```present block (a diagram
//! spec) at the end of a butler reply; that JSON must never be synthesized to
//! audio, but the client still needs it verbatim to render the diagram.
//!
//! Deltas arrive token-by-token, so a ``` marker can straddle two deltas — a
//! trailing run of 1–2 backticks is *carried* until the next delta decides
//! whether it completes a fence. `strip_markdown` only drops COMPLETE fences,
//! so a fence spanning sentences/deltas would otherwise leak to TTS; this gate
//! removes fenced spans as they stream, before the sentence splitter sees them.

/// Streaming fence filter. Feed each brain delta through [`FenceGate::push`];
/// only the speakable portion (text outside ``` fences) comes back out.
#[derive(Default)]
pub struct FenceGate {
    in_fence: bool,
    /// Count of trailing backticks (1–2) not yet resolved into a ``` marker.
    carry: usize,
}

impl FenceGate {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one delta; returns only the text safe to speak. Fence markers and
    /// everything between them are dropped. A partial backtick run at the end
    /// is held (see `carry`) so a ``` split across deltas is still detected.
    pub fn push(&mut self, delta: &str) -> String {
        let mut out = String::new();
        // Re-materialize carried backticks in front of this delta so a marker
        // split across deltas is seen as one contiguous run.
        let mut run = self.carry;
        self.carry = 0;
        for c in delta.chars() {
            if c == '`' {
                run += 1;
                if run == 3 {
                    self.in_fence = !self.in_fence; // ``` toggles in/out
                    run = 0; // the marker is consumed, never spoken
                }
                continue;
            }
            // A non-backtick ends the run. Outside a fence, a run of 1–2 was
            // literal inline-code punctuation — restore it (strip_markdown
            // drops it later anyway); inside a fence it is content, so drop it.
            if run > 0 && !self.in_fence {
                out.extend(std::iter::repeat_n('`', run));
            }
            run = 0;
            if !self.in_fence {
                out.push(c);
            }
        }
        // A trailing 1–2 backtick run is ambiguous — it may open/close a fence
        // once the next delta lands, so hold it rather than speak it.
        self.carry = run;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fenced_span_opened_and_closed_across_deltas_is_never_spoken() {
        let mut g = FenceGate::new();
        assert_eq!(g.push("Here is the plan. ```present"), "Here is the plan. ");
        assert_eq!(g.push("{\"type\":\"flow\","), "", "inside fence — dropped");
        assert_eq!(g.push("\"title\":\"X\"}"), "", "still inside — dropped");
        assert_eq!(g.push("```Thanks."), "Thanks.", "closes, tail spoken");
    }

    #[test]
    fn text_before_and_after_a_fence_survives_intact() {
        let mut g = FenceGate::new();
        // Whole fence in one delta: prose on both sides is kept, JSON removed.
        assert_eq!(
            g.push("Before. ```present\n{\"a\":1}\n``` After."),
            "Before.  After."
        );
    }

    #[test]
    fn a_marker_split_across_two_deltas_is_still_detected() {
        let mut g = FenceGate::new();
        // First delta ends on two backticks (ambiguous — carried, not spoken).
        assert_eq!(g.push("Intro ``"), "Intro ");
        // The third backtick arrives next; the fence opens and its body drops.
        assert_eq!(
            g.push("`json\n{\"n\":1}"),
            "",
            "fence detected despite split"
        );
    }

    #[test]
    fn lone_backticks_that_never_form_a_fence_are_kept() {
        let mut g = FenceGate::new();
        assert_eq!(g.push("use `cargo` now."), "use `cargo` now.");
    }
}
