//! The map is process-global, so these tests share it. Each uses its own
//! platform prefix rather than clearing between runs — a reset helper would be
//! test-only surface on a module whose whole job is to outlive one request.

use super::*;

#[test]
fn the_newest_message_wins_for_a_chat() {
    remember("t1", "c1", "m1");
    assert_eq!(latest("t1", "c1").as_deref(), Some("m1"));
    remember("t1", "c1", "m2");
    assert_eq!(latest("t1", "c1").as_deref(), Some("m2"));
}

/// Two platforms can legitimately use the same chat id. Keying on the id alone
/// would react to a Slack message with a WhatsApp id.
#[test]
fn chats_are_keyed_by_platform_as_well_as_id() {
    remember("t2a", "shared", "from-a");
    remember("t2b", "shared", "from-b");
    assert_eq!(latest("t2a", "shared").as_deref(), Some("from-a"));
    assert_eq!(latest("t2b", "shared").as_deref(), Some("from-b"));
}

#[test]
fn an_unseen_chat_has_nothing_rather_than_a_stale_id() {
    assert_eq!(latest("t3", "never-spoke"), None);
}

/// The bound is the point: without it a long-running gateway grows a map entry
/// per chat forever, for a hint only the newest message needs.
#[test]
fn the_map_stays_bounded_and_drops_the_oldest_first() {
    for i in 0..(MAX_CHATS + 10) {
        remember("t4", &format!("chat{i}"), &format!("m{i}"));
    }
    let (map, order) = &*seen().lock().expect("poisoned");
    assert!(order.len() <= MAX_CHATS, "eviction did not run");
    assert!(map.len() <= MAX_CHATS);
    // The most recent chat survives; the very first does not.
    assert!(map.contains_key(&format!("t4:chat{}", MAX_CHATS + 9)));
    assert!(!map.contains_key("t4:chat0"));
}
