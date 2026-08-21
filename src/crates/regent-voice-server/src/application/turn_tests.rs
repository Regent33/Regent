//! What Regent SAYS when a question card goes up. This is the only half of the
//! feature a caller who is not looking at the screen ever gets, so a silent
//! `None` is a dropped question, not a cosmetic miss.

use super::spoken_question;
use serde_json::json;

#[test]
fn options_are_read_out_the_way_a_person_offers_a_choice() {
    let said = spoken_question(&json!({
        "id": "q_1",
        "questions": [{
            "id": "a",
            "prompt": "Which format?",
            "kind": "single_select",
            "options": [
                {"id": "p", "label": "PDF"},
                {"id": "d", "label": "a Word document"},
                {"id": "s", "label": "slides"},
            ],
        }],
    }));
    assert_eq!(
        said.as_deref(),
        Some("Which format? PDF, a Word document, or slides.")
    );
}

#[test]
fn a_single_option_gets_no_dangling_or() {
    let said = spoken_question(&json!({
        "id": "q_1",
        "questions": [{"id": "a", "prompt": "Ready?", "options": [{"id": "y", "label": "yes"}]}],
    }));
    assert_eq!(said.as_deref(), Some("Ready? yes."));
}

#[test]
fn a_question_with_no_options_is_spoken_as_itself() {
    let said = spoken_question(&json!({
        "id": "q_1",
        "questions": [{"id": "a", "prompt": "What should I call it?", "kind": "text"}],
    }));
    assert_eq!(said.as_deref(), Some("What should I call it?"));
}

/// Only the FIRST question is spoken: the card steps through the rest, and
/// reading three questions in a row out loud is not a conversation.
#[test]
fn only_the_first_question_is_spoken() {
    let said = spoken_question(&json!({
        "id": "q_1",
        "questions": [
            {"id": "a", "prompt": "First?"},
            {"id": "b", "prompt": "Second?"},
        ],
    }));
    assert_eq!(said.as_deref(), Some("First?"));
}

/// A malformed payload must produce silence, never JSON read aloud.
#[test]
fn malformed_payloads_are_silent_rather_than_spoken() {
    for bad in [
        json!({}),
        json!({"questions": []}),
        json!({"questions": [{"id": "a"}]}),
        json!({"questions": [{"id": "a", "prompt": "   "}]}),
        json!("not an object"),
    ] {
        assert_eq!(spoken_question(&bad), None, "should stay silent for {bad}");
    }
}

/// Options without a usable label are skipped rather than voiced as gaps.
#[test]
fn unlabelled_options_are_left_out_of_the_spoken_list() {
    let said = spoken_question(&json!({
        "id": "q_1",
        "questions": [{
            "id": "a",
            "prompt": "Pick one.",
            "options": [{"id": "x", "label": "one"}, {"id": "y"}, {"id": "z", "label": "two"}],
        }],
    }));
    assert_eq!(said.as_deref(), Some("Pick one. one, or two."));
}
