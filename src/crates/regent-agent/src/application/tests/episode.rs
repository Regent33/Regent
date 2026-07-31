//! The episode summary's selection rule. Pure — no store, no graph, no model.

use super::{MAX_SUMMARY_CHARS, MIN_SUMMARY_CHARS, extractive_summary};
use regent_kernel::ChatMessage;

fn assistant(text: &str) -> ChatMessage {
    ChatMessage::assistant(Some(text.to_owned()), Vec::new())
}

#[test]
fn the_summary_is_the_users_asks_in_order() {
    // The assistant's replies are deliberately excluded: a later "what did we
    // do about the deck" query is phrased the way the USER phrases things, so
    // that is what the episode has to match on.
    let summary = extractive_summary(&[
        ChatMessage::user("make a deck about Alice in Wonderland"),
        assistant("Here you go."),
        ChatMessage::user("now add the box office numbers"),
    ]);
    assert_eq!(
        summary,
        "make a deck about Alice in Wonderland\n\nnow add the box office numbers"
    );
}

#[test]
fn a_bare_greeting_anchors_nothing() {
    // A session that is just "hi" is noise in the graph, not history — the
    // caller drops anything under MIN_SUMMARY_CHARS.
    let summary = extractive_summary(&[ChatMessage::user("hi")]);
    assert!(summary.len() < MIN_SUMMARY_CHARS);
}

#[test]
fn truncation_stops_on_a_message_boundary_never_mid_sentence() {
    // A half-sentence embeds badly and reads worse, so the bound is applied by
    // dropping the message that would overflow — not by cutting it.
    // Two that fit together (a third of the budget each, plus the separator),
    // then one that cannot. Sized so the boundary is genuinely exercised: at
    // MAX/2 each, two messages plus "\n\n" already overflow and the second
    // would be dropped, which tests nothing about the third.
    let long = "x".repeat(MAX_SUMMARY_CHARS / 3);
    let overflowing = format!(
        "this last one cannot fit {}",
        "y".repeat(MAX_SUMMARY_CHARS / 2)
    );
    let summary = extractive_summary(&[
        ChatMessage::user(&long),
        ChatMessage::user(&long),
        ChatMessage::user(&overflowing),
    ]);
    assert!(summary.len() <= MAX_SUMMARY_CHARS);
    assert!(
        !summary.contains("this last one cannot fit"),
        "the overflowing message must be dropped whole"
    );
    assert_eq!(summary.matches("\n\n").count(), 1, "two messages survived");
}

#[test]
fn empty_and_whitespace_messages_are_skipped() {
    let summary = extractive_summary(&[
        ChatMessage::user("   "),
        ChatMessage::user("a real question about the renderer"),
    ]);
    assert_eq!(summary, "a real question about the renderer");
}

#[test]
fn a_transcript_with_no_user_messages_yields_nothing() {
    assert!(extractive_summary(&[assistant("unprompted")]).is_empty());
}
