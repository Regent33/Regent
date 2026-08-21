//! Emoji → platform reaction *name*, for the platforms that do not take the
//! character itself.
//!
//! Two of them do this, for different reasons, so the mapping lives here rather
//! than twice: Slack's `reactions.add` takes a shortcode (`thumbsup`) out of a
//! workspace-configurable set, and Messenger's `sender_action: "react"` takes
//! one of exactly seven fixed values. Both are runtime-only failures otherwise
//! — the same class of bug `nearest_allowed` turned into a list plus a test for
//! Telegram.

/// Emoji → Slack shortcode, without the surrounding colons (`reactions.add`
/// rejects `:thumbsup:`). Covers what a model actually reaches for to say
/// "seen", "done", "yes" or "no"; anything unlisted falls back to `+1`, which
/// exists in every Slack workspace by default.
const SLACK: &[(&str, &str)] = &[
    ("👍", "+1"),
    ("👎", "-1"),
    ("❤", "heart"),
    ("🔥", "fire"),
    ("🎉", "tada"),
    ("👀", "eyes"),
    ("✅", "white_check_mark"),
    ("☑", "ballot_box_with_check"),
    ("✔", "heavy_check_mark"),
    ("🙏", "pray"),
    ("👏", "clap"),
    ("😀", "grinning"),
    ("😁", "grin"),
    ("😂", "joy"),
    ("🤣", "rolling_on_the_floor_laughing"),
    ("😊", "blush"),
    ("😍", "heart_eyes"),
    ("🤔", "thinking_face"),
    ("😢", "cry"),
    ("😭", "sob"),
    ("😮", "open_mouth"),
    ("😱", "scream"),
    ("🤯", "exploding_head"),
    ("💯", "100"),
    ("🚀", "rocket"),
    ("⚡", "zap"),
    ("💡", "bulb"),
    ("🐛", "bug"),
    ("👋", "wave"),
    ("🤝", "handshake"),
    ("🙌", "raised_hands"),
    ("💪", "muscle"),
    ("🥳", "partying_face"),
    ("😴", "sleeping"),
    ("🫡", "saluting_face"),
];

/// Emoji → Messenger reaction. The Send API accepts **only** these seven
/// values, so everything else has to be resolved onto one of them or it is a
/// 400. The groupings are by what the reaction MEANS, which is what the model
/// was reaching for: any laugh is `smile`, any upset is `sad`, any shock is
/// `wow`.
const MESSENGER: &[(&str, &str)] = &[
    ("👍", "like"),
    ("👎", "dislike"),
    ("❤", "love"),
    ("😍", "love"),
    ("🥰", "love"),
    ("💯", "love"),
    ("🔥", "love"),
    ("😀", "smile"),
    ("😁", "smile"),
    ("😂", "smile"),
    ("🤣", "smile"),
    ("😊", "smile"),
    ("🎉", "smile"),
    ("😢", "sad"),
    ("😭", "sad"),
    ("💔", "sad"),
    ("😡", "angry"),
    ("😠", "angry"),
    ("🤬", "angry"),
    ("😮", "wow"),
    ("😱", "wow"),
    ("🤯", "wow"),
    ("👀", "wow"),
];

/// What Messenger gets when nothing matches — the neutral acknowledgement,
/// which is what `react_to_message` is usually being asked for.
const MESSENGER_FALLBACK: &str = "like";
/// Slack's universal default; present in every workspace without setup.
const SLACK_FALLBACK: &str = "+1";

/// Slack shortcode for `emoji` (no colons), falling back to `+1`.
#[must_use]
pub fn slack_name(emoji: &str) -> &'static str {
    lookup(SLACK, emoji).unwrap_or(SLACK_FALLBACK)
}

/// Messenger reaction for `emoji`, falling back to `like`.
#[must_use]
pub fn messenger_name(emoji: &str) -> &'static str {
    lookup(MESSENGER, emoji).unwrap_or(MESSENGER_FALLBACK)
}

/// Exact match first, then again with presentation selectors and skin tones
/// dropped — `❤️` and `👍🏽` are what models actually emit, and neither is what
/// a table keyed by the bare character contains.
fn lookup(table: &[(&str, &'static str)], emoji: &str) -> Option<&'static str> {
    if let Some((_, name)) = table.iter().find(|(e, _)| *e == emoji) {
        return Some(name);
    }
    let bare = strip_modifiers(emoji);
    table
        .iter()
        .find(|(e, _)| strip_modifiers(e) == bare)
        .map(|(_, name)| *name)
}

/// Drops variation selectors and skin-tone modifiers. Zero-width joiners are
/// KEPT — `👨‍💻` and `👨` are different reactions, so collapsing them would
/// answer a different question than the one asked.
fn strip_modifiers(emoji: &str) -> String {
    emoji
        .chars()
        .filter(|c| !matches!(*c, '\u{FE0E}' | '\u{FE0F}' | '\u{1F3FB}'..='\u{1F3FF}'))
        .collect()
}

/// Percent-encodes every byte, for a URL **path segment** (Discord's reaction
/// endpoint puts the emoji in the path). Encoding unconditionally is correct
/// here rather than lazy: `validate_emoji` has already rejected every ASCII
/// character, so each remaining byte is non-ASCII and must be encoded anyway.
#[must_use]
pub fn percent_encode_path(value: &str) -> String {
    value.bytes().fold(String::new(), |mut out, byte| {
        out.push_str(&format!("%{byte:02X}"));
        out
    })
}

#[cfg(test)]
#[path = "reaction_names_tests.rs"]
mod tests;
