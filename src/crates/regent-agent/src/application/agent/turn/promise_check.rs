//! Detecting the answer that announces work and then stops.
//!
//! `output_check`'s reveal-on-stuck net only fires on EMPTY assistant content.
//! A weak model in a light-profile session does something worse: it writes
//! "I'll create a comprehensive PPTX presentation and a Word document…", emits
//! no tool call at all, and the turn completes as `outcome="ok"`. Nothing is
//! created, and nothing in the transcript says so.
//!
//! Observed 2026-07-30 on `nvidia/nemotron-3-ultra-550b-a55b`: one API call,
//! one sentence, no files. Earlier the same day the same session logged
//! `revealed=35` from the empty-content path — 35 of ~48 tools were withheld,
//! `create_document` and `write_file` among them, because the light profile
//! pins only 13. The model never made the `load_tools` hop, which is the whole
//! premise deferral rests on.

/// Longest content still treated as a bare announcement.
///
/// A delivered answer that happens to contain "I'll" is not this bug; a
/// one-sentence promise is. The reported case was ~130 chars, and a
/// two-sentence variant ("…Let me start by gathering the details.") lands
/// under 200, so this leaves real headroom without reaching answers of
/// substance.
const PROMISE_MAX_CHARS: usize = 400;

/// Openers that announce imminent work. Matched anywhere in the (already short)
/// content rather than only at the start, so "Sure, I'll create that" counts.
const PROMISES: &[&str] = &[
    "i'll ",
    "i will ",
    "let me ",
    "i'm going to ",
    "i am going to ",
    "i'm creating ",
    "i am creating ",
    "here's what i'll ",
];

/// True when `content` reads as a promise of work rather than a delivered
/// answer. Pure so the phrase list is testable without a provider.
///
/// Deliberately cheap and English-only. A false positive costs exactly one
/// extra model call — the repair prompt permits "or give the user a final
/// answer", so a genuine clarifying question simply gets re-asked — while a
/// false negative costs the user a file that was promised and never written.
pub(super) fn promises_action(content: &str) -> bool {
    let trimmed = content.trim();
    if trimmed.is_empty() || trimmed.chars().count() > PROMISE_MAX_CHARS {
        return false;
    }
    // "Let me know if…" is a sign-off, not a promise, and it is far too common
    // to leave matching "let me ". Removed before the search so a real promise
    // in the same breath ("I'll build the deck. Let me know if…") still counts.
    let lower = trimmed.to_lowercase().replace("let me know", "");
    PROMISES.iter().any(|phrase| lower.contains(phrase))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reported case, verbatim from the screenshot.
    #[test]
    fn the_reported_promise_is_caught() {
        assert!(promises_action(
            "I'll create a comprehensive PPTX presentation and a Word document about \
             Black Panther: Wakanda Forever with all the details you requested."
        ));
    }

    #[test]
    fn promises_in_several_shapes_are_caught() {
        assert!(promises_action("Sure, I'll get that done for you."));
        assert!(promises_action("Let me create that presentation now."));
        assert!(promises_action("I'm going to write the file."));
        assert!(promises_action("I will generate the deck."));
    }

    #[test]
    fn sign_offs_and_delivered_answers_are_not_promises() {
        assert!(
            !promises_action("Let me know if you need anything else."),
            "the single most common closing line must never trigger a retry"
        );
        assert!(!promises_action("The capital of Japan is Tokyo."));
        assert!(!promises_action(""));
        assert!(!promises_action("   \n  "));
    }

    /// A long answer mentioning future work has still ANSWERED — retrying it
    /// would throw away real output.
    #[test]
    fn a_long_answer_is_never_treated_as_a_bare_promise() {
        let delivered = format!(
            "Here are the details you asked for. {} \
             If you want a deck as well, I'll put one together next.",
            "Wakanda Forever was directed by Ryan Coogler and released in 2022. ".repeat(12)
        );
        assert!(delivered.chars().count() > PROMISE_MAX_CHARS);
        assert!(!promises_action(&delivered));
    }
}
