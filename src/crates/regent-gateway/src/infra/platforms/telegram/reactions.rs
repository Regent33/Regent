//! Telegram reactions: the pure half of `setMessageReaction`.
//!
//! Telegram does not accept an arbitrary emoji. `ReactionTypeEmoji` is a fixed
//! enum, and anything outside it is a `400 REACTION_INVALID` — a runtime-only
//! failure, which is exactly the kind this repo prefers to turn into a
//! compile-time list plus a test.

use serde_json::{Value, json};

/// The emoji Telegram accepts as a message reaction (Bot API 7.x
/// `ReactionTypeEmoji`), minus the handful of abusive ones Telegram allows and
/// Regent will not send on a user's behalf. Everything else is kept verbatim,
/// because a trimmed list silently downgrades reactions that would have worked
/// — the omissions here are a deliberate conduct choice, not an oversight.
pub const ALLOWED: &[&str] = &[
    "👍",
    "👎",
    "❤",
    "🔥",
    "🥰",
    "👏",
    "😁",
    "🤔",
    "🤯",
    "😱",
    "😢",
    "🎉",
    "🤩",
    "🤮",
    "💩",
    "🙏",
    "👌",
    "🕊",
    "🤡",
    "🥱",
    "🥴",
    "😍",
    "🐳",
    "❤‍🔥",
    "🌚",
    "🌭",
    "💯",
    "🤣",
    "⚡",
    "🍌",
    "🏆",
    "💔",
    "🤨",
    "😐",
    "🍓",
    "🍾",
    "💋",
    "😈",
    "😴",
    "😭",
    "🤓",
    "👻",
    "👨‍💻",
    "👀",
    "🎃",
    "🙈",
    "😇",
    "😨",
    "🤝",
    "✍",
    "🤗",
    "🫡",
    "🎅",
    "🎄",
    "☃",
    "💅",
    "🤪",
    "🗿",
    "🆒",
    "💘",
    "🙉",
    "🦄",
    "😘",
    "💊",
    "🙊",
    "😎",
    "👾",
    "🤷‍♂",
    "🤷",
    "🤷‍♀",
    "😡",
];

/// What Telegram gets sent for a requested emoji.
///
/// Emoji carry optional presentation selectors (U+FE0F) and skin-tone
/// modifiers that Telegram's enum does not include, so `❤️` must become `❤`
/// rather than being rejected. Anything genuinely outside the set falls back
/// to 👍 — a reaction the user can see beats a silent no-op, and the model
/// already told them in words what it meant.
#[must_use]
pub fn nearest_allowed(emoji: &str) -> &'static str {
    if let Some(exact) = ALLOWED.iter().find(|a| **a == emoji) {
        return exact;
    }
    let bare = strip_modifiers(emoji);
    ALLOWED
        .iter()
        .find(|a| strip_modifiers(a) == bare)
        .copied()
        .unwrap_or("👍")
}

/// Drops variation selectors and skin-tone modifiers so `❤️` and `❤` compare
/// equal. Zero-width joiners are KEPT — `👨‍💻` and `👨` are different reactions.
fn strip_modifiers(emoji: &str) -> String {
    emoji
        .chars()
        .filter(|c| !matches!(*c, '\u{FE0E}' | '\u{FE0F}' | '\u{1F3FB}'..='\u{1F3FF}'))
        .collect()
}

/// The `setMessageReaction` payload. `is_big` is left off — a reaction that
/// fills the screen is the user's choice to make, not the agent's.
#[must_use]
pub fn set_reaction_payload(chat_id: &str, message_id: &str, emoji: &str) -> Value {
    json!({
        "chat_id": chat_id,
        "message_id": message_id.parse::<i64>().unwrap_or_default(),
        "reaction": [{"type": "emoji", "emoji": nearest_allowed(emoji)}],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowed_emoji_pass_through_unchanged() {
        for emoji in ["👍", "🎉", "🤯", "👨‍💻"] {
            assert_eq!(nearest_allowed(emoji), emoji);
        }
    }

    #[test]
    fn presentation_selectors_and_skin_tones_resolve_instead_of_failing() {
        // The forms a model actually emits. Before this, every one was a 400.
        assert_eq!(nearest_allowed("❤️"), "❤");
        assert_eq!(nearest_allowed("✍️"), "✍");
        assert_eq!(nearest_allowed("🕊️"), "🕊");
        assert_eq!(nearest_allowed("👍🏽"), "👍");
        assert_eq!(nearest_allowed("🤷‍♀️"), "🤷‍♀");
    }

    #[test]
    fn anything_telegram_would_reject_falls_back_to_a_visible_reaction() {
        for emoji in ["🥑", "🦆", "🧿"] {
            assert_eq!(nearest_allowed(emoji), "👍", "{emoji}");
        }
    }

    #[test]
    fn zwj_sequences_are_not_collapsed_onto_their_base() {
        // 👨‍💻 is allowed and must stay itself, not become 👨 (which is not).
        assert_eq!(nearest_allowed("👨‍💻"), "👨‍💻");
    }

    #[test]
    fn payload_shape_matches_the_bot_api() {
        let payload = set_reaction_payload("-100123", "77", "❤️");
        assert_eq!(payload["chat_id"], "-100123");
        assert_eq!(payload["message_id"], 77);
        assert_eq!(payload["reaction"][0]["type"], "emoji");
        assert_eq!(payload["reaction"][0]["emoji"], "❤");
    }

    #[test]
    fn the_allowed_list_has_no_duplicates() {
        let mut seen: Vec<&str> = ALLOWED.to_vec();
        seen.sort_unstable();
        let before = seen.len();
        seen.dedup();
        assert_eq!(before, seen.len(), "duplicate entry in ALLOWED");
    }
}
