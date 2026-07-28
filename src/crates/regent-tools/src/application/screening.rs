//! W8 — recording injection-shaped tool results. **Changes nothing.**
//!
//! Tool results were the one unscreened path into the model: memory writes have
//! been scanned since `regent_graph::domain::policy`, but a web page, a
//! platform message, or a file read went straight into the transcript with
//! nothing looking at it.
//!
//! This looks. It does not act, for a reason worth stating before someone
//! "finishes" it:
//!
//! - **Blocking would break Regent reading its own source.** Every marker in
//!   `regent_kernel::threat` appears in that file, in this one, and across
//!   `docs/incidents/2026-07-27-jailed-shell/`. A scanner that stops the agent
//!   opening its own security code is a denial of service you shipped yourself.
//! - **Annotating costs prompt bytes on every hit**, and nothing has measured
//!   how often a hit is a real attack versus a file that discusses attacks.
//!   W3 shipped a shadow pass before it changed a prompt for exactly this
//!   reason, and the number it produced killed the plan's next three steps.
//!
//! ## Exactly what is and is not claimed [co-audit]
//!
//! - The result is returned **byte-identical to what this function received**.
//!   That is the catalog's already-truncated result: `truncate_oversized` runs
//!   first, so the *raw* tool output may already differ. This does not add to
//!   that.
//! - It records **a narrow set of ASCII phrases** plus the presence of
//!   invisible characters — not "anything that reads like an injection".
//!   Homoglyphs, fullwidth forms and decomposed sequences pass straight
//!   through, and nothing here normalizes.
//! - The log is a **count of hits, not a false-positive rate**. It carries no
//!   excerpt and no correlation id, deliberately (tool results contain the
//!   user's data). Turning hits into an FPR needs someone to label them.
//!
//! This is **one layer** of the capability model (ADR-042), and the weakest of
//! them. Pattern matching over text is not a prompt-injection boundary.

use crate::domain::entities::ToolContext;

/// How much of a result to scan. `to_lowercase` allocates a full copy, and a
/// spilled result on every tool call is a real cost for a detector.
const SCAN_LIMIT: usize = 64 * 1024;

/// What a scan found. Separated from the logging so detection is testable on
/// its own — the earlier tests asserted only that the result came back
/// unchanged, and would have passed with the detector deleted entirely
/// [co-audit].
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Shape {
    pub(super) marker: Option<&'static str>,
    pub(super) invisible: bool,
}

impl Shape {
    fn is_clean(&self) -> bool {
        self.marker.is_none() && !self.invisible
    }
}

/// Scans the head of `text`. Pure.
pub(super) fn shape_of(text: &str) -> Shape {
    // Overlap by the longest phrase, or a marker straddling the cutoff is
    // missed — the previous version scanned exactly SCAN_LIMIT bytes and a
    // phrase beginning one byte before it vanished [co-audit].
    let mut end = text
        .len()
        .min(SCAN_LIMIT + regent_kernel::threat::longest_marker_len());
    // Back off to a char boundary: tool results are arbitrary UTF-8, and
    // slicing mid-codepoint panics — inside a scanner, on attacker-influenced
    // bytes, which is the worst possible place to find that out.
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let head = &text[..end];
    Shape {
        marker: regent_kernel::threat::first_detect_marker(head),
        // The matcher above is defeated by a zero-width character inside a
        // phrase, so the characters that defeat it are themselves the signal.
        invisible: head.chars().any(regent_kernel::is_invisible_or_control),
    }
}

/// Records a tool result that reads like an injection attempt. Returns the
/// result unchanged — always.
#[must_use]
pub fn record_injection_shape(tool: &str, result: String, ctx: &ToolContext) -> String {
    let shape = shape_of(&result);
    if !shape.is_clean() {
        tracing::warn!(
            target: "threat_scan",
            tool,
            marker = shape.marker.unwrap_or("-"),
            invisible_chars = shape.invisible,
            untrusted_session = ctx.is_untrusted(),
            // Bytes, not chars: `chars().count()` walks the whole result and
            // would uncap work this function promises to bound [co-audit].
            result_bytes = result.len(),
            "tool result contains injection-shaped text (recorded, not blocked)"
        );
    }
    result
}

#[cfg(test)]
#[path = "tests/screening.rs"]
mod tests;
