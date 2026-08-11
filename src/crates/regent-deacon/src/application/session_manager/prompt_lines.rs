//! Tier-0/1 prompt segments and the Tier-1 ceiling: the timestamp,
//! artifacts, and voice lines every session prompt carries, plus the cap that
//! keeps stacked read-side stores honest. Split from `build.rs` (file-size
//! rule).

use crate::domain::ledger::{Segment, Tier};
use regent_agent::VISUAL_EXPLAINER;

/// Read-side Tier-1 ceiling (SPL §3.4, the ECC cap pattern from §3.7): even
/// when every store sits at its own budget, the SESSION tier injects at most
/// this many bytes — three maxed stores can't stack.
///
/// This is the SUM of the per-store budgets plus the framing they render with,
/// not a round number someone liked. Measured by the test rather than argued
/// for: persona 36,400 (`regent_store::persona_budget` — constitution 12k,
/// soul 8k, about 6k, five `about.<facet>` rows at 2k each, plus its section
/// headings) + skills index 5,416 (24 lines, each hook capped at 140 chars) +
/// graph memory 4,233 (its own per-store limits plus rules and percentage
/// banners) = 46,049. The ~2k over that is headroom, not slack: it absorbs a
/// heading or a banner being reworded without turning a copy edit into a
/// silently truncated prompt.
///
/// The figure this replaced was 36,000 with a comment claiming personas cost
/// 28k, which put the ceiling *at* persona's own budget: a persona written to
/// exactly the limit the write gate advertises consumed all of Tier 1, and the
/// trim below then deleted the skills index and the memory block outright,
/// with nothing said. A ceiling below the budgets it is meant to sit above is
/// not a backstop, it is a silent data loss. Raise a store budget and you must
/// raise this; `tests/prompt_lines.rs` redoes the arithmetic from the real
/// constants and fails if you don't.
///
/// Bytes — and every store budget this sums is now counted in bytes too. That
/// agreement is the whole point. While `persona_budget` counted CHARS and this
/// counted bytes, a persona in a 3-byte script written to exactly the size the
/// CLI accepted arrived here at three times its budgeted size, blew the
/// ceiling on its own, and the trim below deleted the memory block and the
/// skills index — the same silent loss described above, just reachable only by
/// users writing Japanese, Chinese, Korean, Cyrillic, Greek, Arabic or Hebrew.
/// Two units for one invariant is how that hid. If you add a store to this sum,
/// measure it in bytes.
pub(super) const TIER1_CEILING_BYTES: usize = 48_000;

/// Trims Tier-1 segments to the ceiling, walking from the END — later
/// segments (memory, skills index) are retrievable on demand via
/// `memory_search`/`skills_list`, while persona renders first and is trimmed
/// last. A partially-trimmed segment gets a marker naming the trim; the
/// marker's ~0.1k overshoot is accepted. Tier-0 segments are never touched.
///
/// Every trim warns. Losing the memory block is indistinguishable from having
/// no memories, and losing the skills index from having no skills — the agent
/// then looks unreliable for a reason no log explained, so the log explains it.
pub(super) fn cap_tier1(mut segments: Vec<Segment>) -> Vec<Segment> {
    const MARKER: &str = "\n\n[…session context trimmed at the Tier-1 ceiling — the full \
                          content stays retrievable via memory_search / skills_list]";
    let total: usize = segments
        .iter()
        .filter(|s| s.tier == Tier::Session)
        .map(|s| s.text.len())
        .sum();
    let Some(mut over) = total.checked_sub(TIER1_CEILING_BYTES).filter(|o| *o > 0) else {
        return segments;
    };
    tracing::warn!(
        tier1_bytes = total,
        ceiling = TIER1_CEILING_BYTES,
        over,
        "session prompt is over the Tier-1 ceiling — trimming from the end"
    );
    for seg in segments.iter_mut().rev() {
        if seg.tier != Tier::Session || over == 0 {
            continue;
        }
        if seg.text.len() <= over {
            let lost = seg.text.len();
            over -= lost;
            seg.text.clear();
            tracing::warn!(
                segment = seg.name,
                lost_bytes = lost,
                "Tier-1 ceiling DROPPED this prompt segment entirely"
            );
        } else {
            let mut keep = seg.text.len() - over;
            while !seg.text.is_char_boundary(keep) {
                keep -= 1;
            }
            tracing::warn!(
                segment = seg.name,
                lost_bytes = seg.text.len() - keep,
                kept_bytes = keep,
                "Tier-1 ceiling trimmed the tail of this prompt segment"
            );
            seg.text.truncate(keep);
            seg.text.push_str(MARKER);
            over = 0;
        }
    }
    segments
}

/// "\n\nThe session started …" from the LIVE local clock — injected once at
/// session build so the agent answers date/time immediately without mutating
/// the cached prompt mid-turn. Deliberately NOT the launchers' `REGENT_NOW`
/// env: that's captured once at deacon SPAWN, so a long-lived deacon handed
/// every new session a days-stale date (the bug users hit as "Regent doesn't
/// know the date"). Names itself session-START time and points at the
/// `current_time` tool so long sessions stay honest too.
pub(super) fn now_line() -> String {
    let now = chrono::Local::now()
        .format("%A, %B %e, %Y at %I:%M %p (UTC%:z)")
        .to_string();
    format!(
        "\n\nThis session started {now} — the user's local time. Time has passed since; when \
         the exact present moment matters, call the current_time tool."
    )
}

/// Directive pointing the agent at the per-object artifacts area under the
/// real `$REGENT_HOME` (env, else `~/.regent` — never a cwd-relative guess:
/// an unset env used to silence this line and the agent then invented a
/// `.regent/` folder inside whatever directory the deacon ran from).
pub(super) fn artifacts_line() -> String {
    let dir = crate::application::http_serve::regent_home().join("artifacts");
    format!(
        "\n\nWhen you generate a new standalone artifact or file to send (screenshots included — \
         not edits to the user's existing files), create a dedicated folder for it under {} — one \
         subfolder per object, e.g. {}{}<short-slug>/ — put its files there, and tell the user the \
         path. Never create files elsewhere for these; use the user's working directory only for \
         changes to their existing project.",
        dir.display(),
        dir.display(),
        std::path::MAIN_SEPARATOR,
    )
}

/// Spoken-style directive for live voice calls. The speech server spawns its
/// deacon with `REGENT_VOICE=1`; that session then answers conversationally
/// (read aloud, not on screen). Text chat has no env → empty → unchanged.
pub(super) const VOICE_SESSION_ENV: &str = "REGENT_VOICE";

pub(super) fn voice_flag_enabled(value: Option<&str>) -> bool {
    value.is_some_and(|value| matches!(value.trim(), "1" | "true" | "TRUE" | "yes"))
}

pub(super) fn voice_session_active() -> bool {
    voice_flag_enabled(std::env::var(VOICE_SESSION_ENV).ok().as_deref())
}

pub(super) fn voice_line() -> String {
    if !voice_session_active() {
        return String::new();
    }
    "\n\nYOU ARE ON A LIVE VOICE CALL. Your reply is read aloud by text-to-speech, so TALK, don't \
     write. Sound like a person on the phone: contractions (you're, it's, let's), short everyday \
     words, one idea per sentence, and lead with the point the way you would with a friend. Keep it \
     short for simple things — a sentence or two. But when you're genuinely explaining or teaching \
     something, take the time to cover it FULLY and clearly; don't cut a real explanation short. \
     Either way it stays spoken, not written — short sentences, natural flow, no padding. Don't \
     lecture — no 'firstly / \
     secondly / in conclusion', no 'there are three things', no reciting a textbook definition, no \
     repeating the question back before you answer. Contractions and a warm, casual tone always; \
     dry, formal, or essay phrasing reads as robotic out loud. Even when a diagram is on screen, \
     talk it through naturally — react to it like you're pointing at it, don't read it out. NEVER \
     use markdown, headings, bullet or numbered lists, tables, links, code blocks, a 'References' \
     list, or emoji — none of that can be spoken. If the honest answer is long or list-like (a \
     weather breakdown, search results, many items), say the one-line takeaway and offer to drop \
     the full details in text/chat. Prefer round numbers and plain phrasing over exact figures and \
     jargon. This overrides any formatting guidance above.\
     \n\nScreen vision on a call: when asked whether you can see the screen or what is on it, call \
     computer_use with action=screenshot AND the caller's question. That one read-only call captures \
     and analyzes the screen; it needs no permission, so never claim you cannot see, ask for screen \
     sharing, or ask to proceed. For a named window or browser tab, never press blind alt+f4, cmd+q, \
     or ctrl+w: the Regent call usually has focus and would close itself. Call list_windows, select \
     the exact returned window_id, then close_window; for a tab, list_tabs then close_tab. If the \
     target is ambiguous, ask which one in one short sentence.\
     \n\nLong jobs on a call: for work that needs more than a minute or two — building or fixing \
     software (code_task included), deep research, producing documents, spreadsheets, or \
     presentations — call background_task instead of doing it inline, tell the caller it's \
     started, and keep the conversation going. The result reaches you automatically in a later \
     turn; speak its takeaway then. Never leave the caller waiting in silence for a long job.\n\n"
        .to_owned()
        + VISUAL_EXPLAINER
}

#[cfg(test)]
#[path = "tests/prompt_lines.rs"]
mod tests;
