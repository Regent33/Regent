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
    /// Did the delta just fed close a fence? See [`FenceGate::closed_fence`].
    closed: bool,
    /// Has any speakable text been seen this turn? Only while this is false can
    /// a bare `{` open the unfenced-spec path below.
    spoke: bool,
    /// Brace depth of a bare leading spec object; 0 = not in one.
    depth: usize,
    in_string: bool,
    escaped: bool,
    /// Chars consumed by the bare object, for the runaway bound.
    eaten: usize,
}

/// A diagram spec is a few hundred bytes. If a bare `{` has not balanced by
/// this much, it was not a spec and the gate stops swallowing — otherwise one
/// stray brace would mute the entire turn.
/// ponytail: fixed bound, not a parser. Raise it if a real spec ever grows past
/// this; the failure mode it guards is silence, which is far worse than a
/// spec's opening fragment going unspoken.
const BARE_OBJECT_MAX: usize = 4096;

impl FenceGate {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// True when the delta just fed CLOSED a fenced span. A butler reply leads
    /// with its diagram spec, which yields no speakable text — so the turn loop
    /// emits no `reply` line while it streams, and the client cannot draw the
    /// diagram until a whole spoken sentence has also arrived. This lets the
    /// caller flush the spec the instant it is complete instead.
    #[must_use]
    pub fn closed_fence(&self) -> bool {
        self.closed
    }

    /// Feed one delta; returns only the text safe to speak. Fence markers and
    /// everything between them are dropped. A partial backtick run at the end
    /// is held (see `carry`) so a ``` split across deltas is still detected.
    pub fn push(&mut self, delta: &str) -> String {
        self.closed = false;
        let mut out = String::new();
        // Re-materialize carried backticks in front of this delta so a marker
        // split across deltas is seen as one contiguous run.
        let mut run = self.carry;
        self.carry = 0;
        for c in delta.chars() {
            // A weak voice model drops the ``` and emits the spec as a bare
            // leading object. Unfenced, it was neither removed from TTS (the
            // JSON got read aloud) nor flushed early, so the diagram waited for
            // a whole spoken sentence. Handled here, in the shared gate, so
            // both callers — the TTS strip and the early `reply` flush — are
            // fixed by one guard.
            if self.depth > 0 {
                self.eat_object(c);
                continue;
            }
            if c == '`' {
                run += 1;
                if run == 3 {
                    self.in_fence = !self.in_fence; // ``` toggles in/out
                    if !self.in_fence {
                        self.closed = true; // a spec just completed
                    }
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
            if self.in_fence {
                continue;
            }
            if !self.spoke {
                if c == '{' {
                    self.depth = 1;
                    self.eaten = 1;
                    continue;
                }
                if !c.is_whitespace() {
                    self.spoke = true;
                }
            }
            out.push(c);
        }
        // A trailing 1–2 backtick run is ambiguous — it may open/close a fence
        // once the next delta lands, so hold it rather than speak it.
        self.carry = run;
        out
    }

    /// One char of a bare leading object. Braces inside string literals must not
    /// move the depth, so the string/escape state is tracked too — a spec label
    /// like "step {1}" would otherwise unbalance it and mute the turn.
    fn eat_object(&mut self, c: char) {
        self.eaten += 1;
        if self.eaten > BARE_OBJECT_MAX {
            self.abandon_object();
            return;
        }
        if self.escaped {
            self.escaped = false;
            return;
        }
        if self.in_string {
            match c {
                '\\' => self.escaped = true,
                '"' => self.in_string = false,
                _ => {}
            }
            return;
        }
        match c {
            '"' => self.in_string = true,
            '{' => self.depth += 1,
            '}' => {
                self.depth -= 1;
                if self.depth == 0 {
                    // Same signal a closing ``` gives: the spec is complete, so
                    // the caller can flush it now instead of waiting for speech.
                    self.closed = true;
                    self.abandon_object();
                }
            }
            _ => {}
        }
    }

    /// Leave bare-object mode. `spoke` is set either way so a later `{` in the
    /// same turn is ordinary text, never a second spec.
    fn abandon_object(&mut self) {
        self.depth = 0;
        self.in_string = false;
        self.escaped = false;
        self.spoke = true;
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

    /// The leading diagram spec must be announced the moment it completes —
    /// it produces no speakable text, so nothing else would flush it.
    #[test]
    fn closing_a_fence_is_reported_once_on_the_delta_that_closed_it() {
        let mut g = FenceGate::new();
        g.push("```json");
        assert!(!g.closed_fence(), "still open");
        g.push("{\"type\":\"flow\"}");
        assert!(!g.closed_fence(), "still open");
        g.push("```");
        assert!(g.closed_fence(), "the spec is complete");
        g.push(" Now the explanation.");
        assert!(!g.closed_fence(), "not re-reported on later deltas");
    }

    /// Weak voice models drop the ``` and emit the spec as a bare object. It
    /// was spoken aloud AND the diagram waited for a whole sentence.
    #[test]
    fn a_bare_leading_spec_object_is_never_spoken_and_flushes_on_close() {
        let mut g = FenceGate::new();
        assert_eq!(g.push("{\"type\":\"flow\","), "", "bare spec is not speech");
        assert!(!g.closed_fence(), "still open");
        assert_eq!(g.push("\"title\":\"X\"}"), "", "still the spec");
        assert!(
            g.closed_fence(),
            "complete: flush it now, do not wait for speech"
        );
        assert_eq!(g.push(" Here is how it works."), " Here is how it works.");
        assert!(!g.closed_fence(), "not re-reported");
    }

    #[test]
    fn a_brace_inside_a_spec_label_does_not_unbalance_it() {
        let mut g = FenceGate::new();
        // Braces and an escaped quote INSIDE strings must not move the depth.
        assert_eq!(g.push("{\"title\":\"step {1} of \\\"two\\\"\"}"), "");
        assert!(g.closed_fence(), "the object really closed");
        assert_eq!(g.push("Done."), "Done.");
    }

    #[test]
    fn a_brace_after_speech_has_started_is_ordinary_text() {
        let mut g = FenceGate::new();
        assert_eq!(
            g.push("Use the {placeholder} there."),
            "Use the {placeholder} there."
        );
        assert!(!g.closed_fence());
    }

    #[test]
    fn an_object_that_never_closes_cannot_mute_the_whole_turn() {
        let mut g = FenceGate::new();
        assert_eq!(g.push("{ not really a spec "), "");
        // Past the bound the gate gives up and speech resumes.
        let long = "x".repeat(super::BARE_OBJECT_MAX);
        let spoken = g.push(&long);
        assert!(!spoken.is_empty(), "the gate must recover, not stay silent");
        assert_eq!(g.push(" and then some."), " and then some.");
    }
}
