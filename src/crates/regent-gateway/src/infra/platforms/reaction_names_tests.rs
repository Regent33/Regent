//! Every case here is a 400 from a third party if it regresses, which is a
//! failure the user sees as "Regent just didn't react" with nothing in the log.

use super::*;

#[test]
fn slack_maps_the_common_acknowledgements_to_real_shortcodes() {
    assert_eq!(slack_name("👍"), "+1");
    assert_eq!(slack_name("👎"), "-1");
    assert_eq!(slack_name("🎉"), "tada");
    assert_eq!(slack_name("💯"), "100");
}

#[test]
fn messenger_resolves_onto_its_seven_fixed_values() {
    const ALLOWED: [&str; 7] = ["smile", "angry", "sad", "wow", "love", "like", "dislike"];
    for (emoji, _) in MESSENGER {
        assert!(
            ALLOWED.contains(&messenger_name(emoji)),
            "{emoji} maps outside the Send API's set"
        );
    }
    // Anything unlisted still has to land inside the set, not pass through.
    assert!(ALLOWED.contains(&messenger_name("🥑")));
}

#[test]
fn a_laugh_is_a_smile_and_a_shock_is_a_wow() {
    // Messenger has no 🤯; answering with `like` would say the wrong thing.
    assert_eq!(messenger_name("🤣"), "smile");
    assert_eq!(messenger_name("🤯"), "wow");
    assert_eq!(messenger_name("😭"), "sad");
    assert_eq!(messenger_name("🤬"), "angry");
}

/// The forms a model actually emits. Without the modifier strip, every one of
/// these silently became the fallback instead of what was asked for.
#[test]
fn presentation_selectors_and_skin_tones_resolve_rather_than_falling_back() {
    assert_eq!(slack_name("❤️"), "heart");
    assert_eq!(slack_name("👍🏽"), "+1");
    assert_eq!(messenger_name("❤️"), "love");
    assert_eq!(messenger_name("👍🏿"), "like");
}

#[test]
fn an_unknown_emoji_falls_back_to_something_that_always_exists() {
    // A visible reaction beats a 400 the user never sees.
    assert_eq!(slack_name("🦄"), "+1");
    assert_eq!(messenger_name("🦄"), "like");
}

#[test]
fn slack_shortcodes_never_carry_colons() {
    for (_, name) in SLACK {
        assert!(!name.contains(':'), "{name} would be rejected by Slack");
    }
}

#[test]
fn the_tables_have_no_duplicate_keys() {
    for (label, table) in [("slack", SLACK), ("messenger", MESSENGER)] {
        let mut keys: Vec<&str> = table.iter().map(|(e, _)| *e).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(before, keys.len(), "duplicate emoji in the {label} table");
    }
}

#[test]
fn path_encoding_escapes_every_byte_of_an_emoji() {
    // Discord puts this straight into a URL path — a raw 👍 there is a 404 at
    // best, and the reason percent-encoding is a trust boundary and not style.
    assert_eq!(percent_encode_path("👍"), "%F0%9F%91%8D");
    assert_eq!(percent_encode_path("❤"), "%E2%9D%A4");
    assert!(!percent_encode_path("🎉").contains('🎉'));
}
