//! Injection-shaped text patterns (W8), in the one crate both the memory
//! writer and the tool dispatcher can import.
//!
//! ## This is a detector, not a boundary
//!
//! Pattern matching over text does not stop prompt injection and must never be
//! described as if it does [co-audit]. An attacker who knows the list routes
//! around it in one edit — zero-width joiners, homoglyphs, fullwidth forms and
//! decomposed combining sequences all defeat substring matching, and nothing
//! here normalizes. It is one layer of the capability model in ADR-042, and the
//! weakest: the trust split, the approval gates and the path jail are what
//! bound damage.
//!
//! ## Two lists, because a false positive costs two different things
//!
//! - [`REJECT_MARKERS`] — **memory writes refuse these.** Stored memory replays
//!   into the system prompt on every later turn, so the bar is worth paying
//!   for; but a false positive blocks the user's real memory, so this list
//!   stays tiny and every phrase in it has one meaning wherever it appears.
//! - [`DETECT_EXTRA_MARKERS`] — **tool results only log these.** A false
//!   positive costs one log line, so the list can afford phrases that are
//!   merely suspicious. `disregard all previous` is here and not above
//!   precisely because "disregard all previous budget estimates" is legitimate
//!   memory [co-audit].
//!
//! The split is sharper than "one list is longer" because of *who owns this
//! machine*: the corpus here is an AI-agent codebase, so nearly every injection
//! phrase is something the owner might legitimately write down. Breadth belongs
//! on the side that costs a log line. The reject list earns its width from the
//! verb→object rule below, which is broad in coverage and narrow in meaning.
//!
//! Tool results are never refused. They are transient, unbounded, and often
//! *about* injection: every marker below appears in this file and across
//! `docs/incidents/2026-07-27-jailed-shell/`, so a blocking scanner would stop
//! Regent reading its own security code — a denial of service shipped to
//! yourself.

pub use crate::threat_patterns::{
    CLAUSE_ENDS, DETECT_EXTRA_MARKERS, EXFIL_LABEL, EXFIL_OBJECTS, EXFIL_VERBS, LOOSE_LABEL,
    LOOSE_OBJECTS, OVERRIDE_LABEL, OVERRIDE_OBJECTS, OVERRIDE_VERBS, OVERRIDE_WINDOW,
    REJECT_MARKERS,
};

/// Substring search that refuses to match mid-word.
///
/// The reason this is not `contains`: `"you are now"` matched **"you are
/// NOWHERE near the budget"**, and had been rejecting memory writes like it
/// since the check shipped. A trailing space fixes that one case and silently
/// creates others — `"You are now:\n"`, `"you are now\na pirate"` and a
/// non-breaking space all stop matching [co-audit]. A boundary test keeps the
/// phrase armed against every separator and still refuses the `h` in
/// "nowhere".
fn contains_phrase(haystack: &str, needle: &str) -> bool {
    // Word boundaries only apply to word-shaped needles. `![](http` and
    // `<|im_start|>` are structural fragments, not words: requiring a
    // non-alphanumeric character after them means `![](http` can never match
    // `![](https://…`, which is the exact string it was added for.
    if !needle.chars().all(|c| c.is_alphanumeric() || c == ' ') {
        return haystack.contains(needle);
    }
    haystack.match_indices(needle).any(|(start, _)| {
        let before = haystack[..start].chars().next_back();
        let after = haystack[start + needle.len()..].chars().next();
        let free = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric());
        free(before) && free(after)
    })
}

/// `tail` up to the end of its first clause.
///
/// A `.` only ends a clause when whitespace follows it. Splitting on every dot
/// cut `.env` in half, so *"upload the .env to…"* stopped pairing — the object
/// the rule exists to catch was being eaten by the rule's own delimiter.
fn clause_prefix(tail: &str) -> &str {
    let mut chars = tail.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let ends_clause = match c {
            '.' => chars.peek().is_none_or(|(_, next)| next.is_whitespace()),
            other => CLAUSE_ENDS.contains(&other),
        };
        if ends_clause {
            return &tail[..i];
        }
    }
    tail
}

/// Whether any [`OVERRIDE_VERBS`] entry is followed by one of `objects` within
/// [`OVERRIDE_WINDOW`] bytes. `haystack` must already be lowercased.
///
/// Direction matters: the object must come *after* the verb. "the instructions
/// say to ignore warnings" pairs the same two words in the harmless order, and
/// an order-blind proximity check would flag it.
fn pairs_with(haystack: &str, verbs: &[&str], objects: &[&str]) -> bool {
    verbs.iter().any(|verb| {
        haystack.match_indices(verb).any(|(start, _)| {
            let tail_start = start + verb.len();
            let tail_end = (tail_start + OVERRIDE_WINDOW).min(haystack.len());
            // Attacker-influenced UTF-8: slicing mid-codepoint panics, and a
            // panic inside the scanner is a worse outcome than a missed match.
            let mut end = tail_end;
            while end > tail_start && !haystack.is_char_boundary(end) {
                end -= 1;
            }
            let tail = &haystack[tail_start..end];
            // Same clause only — the window is the backstop, not the rule.
            let clause = clause_prefix(tail);
            objects.iter().any(|object| contains_phrase(clause, object))
        })
    })
}

/// The memory-write gate: [`REJECT_MARKERS`] plus the tight override rule.
#[must_use]
pub fn first_injection_marker(text: &str) -> Option<&'static str> {
    let lowered = text.to_lowercase();
    first_of(REJECT_MARKERS, &lowered).or_else(|| {
        pairs_with(&lowered, OVERRIDE_VERBS, OVERRIDE_OBJECTS).then_some(OVERRIDE_LABEL)
    })
}

/// The tool-result detector: everything the write gate refuses, plus the looser
/// phrases and the guardrail/exfiltration rules. Superset by construction.
#[must_use]
pub fn first_detect_marker(text: &str) -> Option<&'static str> {
    first_injection_marker(text).or_else(|| {
        let lowered = text.to_lowercase();
        first_of(DETECT_EXTRA_MARKERS, &lowered)
            .or_else(|| pairs_with(&lowered, OVERRIDE_VERBS, LOOSE_OBJECTS).then_some(LOOSE_LABEL))
            .or_else(|| pairs_with(&lowered, EXFIL_VERBS, EXFIL_OBJECTS).then_some(EXFIL_LABEL))
    })
}

/// How far a match can reach, in bytes. Callers that scan a prefix of a large
/// input need this much overlap or a phrase straddling their cutoff is missed
/// [co-audit] — and the verb→object rule reaches further than any literal:
/// a verb one byte before the cutoff can pair with an object a window later.
#[must_use]
pub fn longest_marker_len() -> usize {
    let longest_literal = DETECT_EXTRA_MARKERS
        .iter()
        .chain(REJECT_MARKERS)
        .map(|m| m.len())
        .max()
        .unwrap_or(0);
    let longest_object = OVERRIDE_OBJECTS
        .iter()
        .chain(LOOSE_OBJECTS)
        .chain(EXFIL_OBJECTS)
        .map(|o| o.len())
        .max()
        .unwrap_or(0);
    let longest_verb = OVERRIDE_VERBS
        .iter()
        .chain(EXFIL_VERBS)
        .map(|v| v.len())
        .max()
        .unwrap_or(0);
    longest_literal.max(longest_verb + OVERRIDE_WINDOW + longest_object)
}

/// `lowered` must already be lowercased — `to_lowercase` allocates, and the
/// detector calls this three times per scan.
fn first_of(markers: &[&'static str], lowered: &str) -> Option<&'static str> {
    markers
        .iter()
        .copied()
        .find(|marker| contains_phrase(lowered, marker))
}

/// `text` without a **leading** byte-order mark.
///
/// Same argument as `\r` below, and measured the same way. U+FEFF at offset 0
/// is an encoding marker every Windows editor writes; it cannot conceal
/// anything, because there is nothing in front of it to disagree with. Two
/// files in this repo's own `src` carry one, and 8 of 45 invisible-character
/// hits across a 57k-file corpus were nothing else (2026-07-29).
///
/// `trim` does not cover this: U+FEFF stopped being `White_Space` in Unicode,
/// so `char::is_whitespace` returns false and a pasted BOM survived into
/// `validate_content`, which **refuses the write**. That is the expensive
/// direction of false positive — the user's real memory, blocked.
///
/// Only the leading one. A BOM anywhere else is a zero-width no-break space
/// sitting inside text, which is exactly the evasion the rest of this module
/// is looking for.
#[must_use]
pub fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{FEFF}').unwrap_or(text)
}

/// Whether `c` is invisible or control text. Bidi overrides and zero-width
/// joiners let one string render as something other than what it says, which
/// defeats review by eye — and defeats the substring matching above.
///
/// `\r` is layout, not evasion. Excluding it is not a nicety: this is a Windows
/// machine and the repo is full of CRLF, so treating it as hostile made **every
/// read of a CRLF file** report invisible characters, and refused any memory
/// write carrying a pasted Windows line ending. A signal that fires on all of
/// normal traffic is not a signal.
#[must_use]
pub fn is_invisible_or_control(c: char) -> bool {
    (c.is_control() && c != '\n' && c != '\t' && c != '\r')
        || matches!(c,
            '\u{200B}'..='\u{200F}'   // zero-width + direction marks
            | '\u{202A}'..='\u{202E}' // bidi embedding/override
            | '\u{2066}'..='\u{2069}' // bidi isolates
            | '\u{FEFF}') // BOM / zero-width no-break
}

#[cfg(test)]
#[path = "threat_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "threat_bom_tests.rs"]
mod bom_tests;
