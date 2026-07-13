//! Tier-0/1 prompt segments and the Tier-1 ceiling: the timestamp,
//! artifacts, and voice lines every session prompt carries, plus the cap that
//! keeps stacked read-side stores honest. Split from `build.rs` (file-size
//! rule).

use crate::domain::ledger::{Segment, Tier};
use regent_agent::VISUAL_EXPLAINER;

/// Read-side Tier-1 ceiling (SPL §3.4, the ECC cap pattern from §3.7): even
/// when every store sits at its own budget, the SESSION tier injects at most
/// this many chars — three maxed stores can't stack. Sized just above the sum
/// of today's per-store budgets (personas 28k + skills index ~4k + memory
/// ~3.6k), so it only bites when something actually stacks past design.
pub(super) const TIER1_CEILING_CHARS: usize = 36_000;

/// Trims Tier-1 segments to the ceiling, walking from the END — later
/// segments (memory, skills index) are retrievable on demand via
/// `memory_search`/`skills_list`, while persona renders first and is trimmed
/// last. A partially-trimmed segment gets a marker naming the trim; the
/// marker's ~0.1k overshoot is accepted. Tier-0 segments are never touched.
pub(super) fn cap_tier1(mut segments: Vec<Segment>) -> Vec<Segment> {
    const MARKER: &str = "\n\n[…session context trimmed at the Tier-1 ceiling — the full \
                          content stays retrievable via memory_search / skills_list]";
    let total: usize = segments
        .iter()
        .filter(|s| s.tier == Tier::Session)
        .map(|s| s.text.len())
        .sum();
    let Some(mut over) = total.checked_sub(TIER1_CEILING_CHARS).filter(|o| *o > 0) else {
        return segments;
    };
    for seg in segments.iter_mut().rev() {
        if seg.tier != Tier::Session || over == 0 {
            continue;
        }
        if seg.text.len() <= over {
            over -= seg.text.len();
            seg.text.clear();
        } else {
            let mut keep = seg.text.len() - over;
            while !seg.text.is_char_boundary(keep) {
                keep -= 1;
            }
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
pub(super) fn voice_line() -> String {
    let on = std::env::var("REGENT_VOICE")
        .map(|v| matches!(v.trim(), "1" | "true" | "TRUE" | "yes"))
        .unwrap_or(false);
    if !on {
        return String::new();
    }
    "\n\nYOU ARE ON A LIVE VOICE CALL. Your reply is read aloud by text-to-speech, so talk like a \
     person on the phone — warm, natural, with contractions. Give the gist in 1-3 short spoken \
     sentences, not a written report. NEVER use markdown, headings, bullet or numbered lists, \
     tables, links, code blocks, a 'References' list, or emoji — none of that can be spoken. If the \
     honest answer is long or list-like (a weather breakdown, search results, many items), say the \
     one-line takeaway and offer to drop the full details in text/chat. Prefer round numbers and \
     plain phrasing over exact figures and jargon. This overrides any formatting guidance above.\
     \n\nControlling the screen by voice: computer_use keys/clicks act on the FOCUSED window, and \
     THIS CALL is running in a browser tab — so a blind 'close this tab' could close the call \
     itself. When the caller says 'this'/'that'/'here' about a window, tab, or app and you can't \
     tell what's in front, take a screenshot first to see the focused window; if it's still \
     ambiguous or you'd be acting on the call tab, ask which one in one short sentence before you \
     act. When it's clearly unambiguous, just do it.\
     \n\nLong jobs on a call: for work that needs more than a minute or two — building or fixing \
     software (code_task included), deep research, producing documents, spreadsheets, or \
     presentations — call background_task instead of doing it inline, tell the caller it's \
     started, and keep the conversation going. The result reaches you automatically in a later \
     turn; speak its takeaway then. Never leave the caller waiting in silence for a long job.\n\n"
        .to_owned()
        + VISUAL_EXPLAINER
}
