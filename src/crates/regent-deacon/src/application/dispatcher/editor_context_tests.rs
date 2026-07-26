//! Unit tests for `editor_context` — the note that tells the agent what the
//! user has open, and which lines they highlighted.

use super::*;
use serde_json::json;

#[test]
fn an_open_file_alone_contributes_a_path_only_note() {
    let note = editor_note(&json!({"open_file": "src/main.rs"})).expect("a note");
    assert!(note.contains("src/main.rs"));
    // Path only: no file body is pulled into the prompt.
    assert!(note.lines().count() <= 3, "should be a one-liner: {note}");
}

#[test]
fn a_selection_carries_its_lines_and_text() {
    let note = editor_note(&json!({
        "open_file": "src/main.rs",
        "selection": {"start_line": 10, "end_line": 12, "text": "fn main() {}"},
    }))
    .expect("a note");
    assert!(note.contains("10-12"), "line range is stated: {note}");
    assert!(note.contains("fn main() {}"), "selected text is included");
}

#[test]
fn no_open_file_means_no_note_at_all() {
    assert!(editor_note(&json!({})).is_none());
    assert!(editor_note(&json!({"open_file": ""})).is_none());
    assert!(editor_note(&json!({"open_file": "   "})).is_none());
}

/// A selection missing any of its parts degrades to the plain open-file note
/// rather than rendering a half-formed range.
#[test]
fn a_malformed_selection_falls_back_to_the_file_note() {
    let note = editor_note(&json!({
        "open_file": "a.ts",
        "selection": {"start_line": 3},
    }))
    .expect("a note");
    assert!(note.contains("a.ts"));
    assert!(!note.contains("selected"), "no partial range: {note}");
}

#[test]
fn an_empty_selection_is_treated_as_no_selection() {
    let note = editor_note(&json!({
        "open_file": "a.ts",
        "selection": {"start_line": 1, "end_line": 1, "text": ""},
    }))
    .expect("a note");
    assert!(
        !note.contains("selected"),
        "empty text isn't a selection: {note}"
    );
}

/// A client sending a whole file as "selection" must not paste it into the
/// prompt — the range still says what was highlighted.
#[test]
fn an_oversized_selection_is_trimmed_not_pasted_whole() {
    let huge = "x".repeat(SELECTION_MAX_CHARS * 2);
    let note = editor_note(&json!({
        "open_file": "big.txt",
        "selection": {"start_line": 1, "end_line": 9000, "text": huge},
    }))
    .expect("a note");
    assert!(note.contains("selection trimmed"));
    assert!(note.chars().count() < SELECTION_MAX_CHARS + 500);
}

#[test]
fn an_absurdly_long_path_is_refused() {
    let path = "a/".repeat(PATH_MAX_CHARS);
    assert!(editor_note(&json!({"open_file": path})).is_none());
}

/// A highlighted folder with no file open still scopes the agent.
#[test]
fn a_selected_folder_alone_is_reported() {
    let note = editor_note(&json!({"open_folder": "src/features"})).expect("a note");
    assert!(note.contains("src/features"));
    assert!(note.contains("folder"));
}

/// An open file wins: naming both would be noise, and the file is the sharper
/// signal (its folder is implied by its path anyway).
#[test]
fn an_open_file_takes_precedence_over_a_selected_folder() {
    let note = editor_note(&json!({"open_file": "src/a.ts", "open_folder": "src"})).expect("note");
    assert!(note.contains("src/a.ts"));
    assert!(
        !note.contains("folder selected"),
        "no duplicate folder line: {note}"
    );
}
