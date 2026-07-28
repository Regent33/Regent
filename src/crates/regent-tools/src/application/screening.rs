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
//! - **Annotating costs prompt bytes on every hit**, and the hit rate is now
//!   measured (below) rather than assumed. W3 shipped a shadow pass before it
//!   changed a prompt for exactly this reason, and the number it produced
//!   killed the plan's next three steps.
//!
//! ## The hit rate, measured 2026-07-29
//!
//! The paragraph that used to sit here said turning hits into a false-positive
//! rate "needs someone to label them". Done, by running this detector over
//! every UTF-8 file in three corpora and reading the hits:
//!
//! | corpus | files | flagged | marker | invisible |
//! |---|---|---|---|---|
//! | this repo's `src` | 1,800 | 1.2% | 0.8% | 0.4% |
//! | this repo's `docs` | 123 | 3.3% | 3.3% | 0% |
//! | an unrelated app repo | 57,069 | 0.6% | 0.5% | 0.1% |
//!
//! **Every marker hit inspected was a false positive** — and the noise is
//! concentrated, not spread: four phrases produced 86% of it. `run the
//! following command` (98) is install-doc prose; `this overrides` (89) is a
//! docstring cliché and has since been **deleted** as redundant with the
//! override rule; `without any restrictions` (35) was pypdf describing PDF
//! permissions; `jailbreak` (15) was the gcloud SDK's own injection-filter
//! flags — another vendor's security code, tripping for the same reason ours
//! does.
//!
//! Read as a per-tool-result rate rather than a corpus ratio, ~1 read in 150
//! records a line it should not. That is usable, and it is the number this
//! module previously did not have. What it is *not* is a rate that survives
//! reading a whole tree: a scan of 57k files buries a real hit in 274.
//!
//! ## Exactly what is and is not claimed [co-audit]
//!
//! - The result is returned **byte-identical to what this function received**.
//!   That is the catalog's already-truncated result: `truncate_oversized` runs
//!   first, so the *raw* tool output may already differ. This does not add to
//!   that.
//! - It records **a narrow set of ASCII phrases** plus the first invisible
//!   character — not "anything that reads like an injection". Homoglyphs,
//!   fullwidth forms and decomposed sequences pass straight through, and
//!   nothing here normalizes.
//! - The log carries no excerpt and no correlation id, deliberately (tool
//!   results contain the user's data). It now names the offending codepoint,
//!   which is the most a reader can be given without the text itself.
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
    /// The **first** invisible character, not a bare flag. A bool made every
    /// hit look alike, and measurement (2026-07-29) showed they are not: a
    /// U+FEFF is a file's byte-order mark, a U+0000 run is a binary log, a
    /// U+200B inside a word is the evasion this exists for. Naming the
    /// codepoint is what lets a reader triage the line without the tool result
    /// — which the log deliberately does not carry.
    pub(super) invisible: Option<char>,
}

impl Shape {
    fn is_clean(&self) -> bool {
        self.marker.is_none() && self.invisible.is_none()
    }
}

/// Scans the head of `text`. Pure.
pub(super) fn shape_of(text: &str) -> Shape {
    // Covers a tool that returns raw text. It does NOT cover `read_file`,
    // whose result is JSON: there the mark sits after `{"content":"`, not at
    // offset 0, and no offset-0 strip can reach it. Verified on real traffic —
    // a BOM-prefixed file read still records, which is why the log now names
    // the codepoint instead of claiming a boolean "evasion".
    let text = regent_kernel::strip_bom(text);
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
        invisible: head
            .chars()
            .find(|c| regent_kernel::is_invisible_or_control(*c)),
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
            invisible_char = shape
                .invisible
                .map_or_else(|| "-".to_owned(), |c| format!("U+{:04X}", c as u32)),
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
