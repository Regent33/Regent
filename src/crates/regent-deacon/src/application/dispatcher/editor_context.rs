//! What the user is looking at in the coding panel, rendered into the turn.
//!
//! Two levels, because they answer different questions:
//!   * the open FILE is a path only — the agent has file tools and the file is
//!     inside its own workspace, so a path costs nothing and still lets it read
//!     whatever it needs;
//!   * a SELECTION carries its text, because "this bit here" is exactly what a
//!     path cannot express.
//!
//! Rendered as a bracketed note appended to the user's text, the same shape the
//! attachment list already uses, so no prompt-assembly contract changes.

use serde_json::Value;

/// Guard against a client sending a whole file as "selection". The line range
/// still tells the agent what was highlighted, and it can read the rest.
const SELECTION_MAX_CHARS: usize = 4000;
/// A path is short by nature; anything longer is malformed or hostile.
const PATH_MAX_CHARS: usize = 1024;

/// Build the editor-context note for a `prompt.submit`, or `None` when the
/// client sent nothing usable. `params` is the whole request params object.
pub(super) fn editor_note(params: &Value) -> Option<String> {
    let path = params
        .get("open_file")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|p| !p.is_empty() && p.chars().count() <= PATH_MAX_CHARS)?;

    let selection = params.get("selection").and_then(|s| {
        let start = s.get("start_line").and_then(Value::as_u64)?;
        let end = s.get("end_line").and_then(Value::as_u64)?;
        let text = s.get("text").and_then(Value::as_str)?;
        if text.is_empty() {
            return None;
        }
        Some((start, end, clip(text)))
    });

    Some(match selection {
        Some((start, end, text)) => format!(
            "\n\n[The user is looking at {path} in the editor and has lines {start}-{end} \
             selected:\n{text}\n]"
        ),
        None => format!("\n\n[The user has {path} open in the editor.]"),
    })
}

fn clip(text: &str) -> String {
    if text.chars().count() <= SELECTION_MAX_CHARS {
        return text.to_owned();
    }
    let mut clipped: String = text.chars().take(SELECTION_MAX_CHARS).collect();
    clipped.push_str("\n… (selection trimmed)");
    clipped
}

#[cfg(test)]
#[path = "editor_context_tests.rs"]
mod tests;
