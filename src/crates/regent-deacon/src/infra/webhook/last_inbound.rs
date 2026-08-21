//! The newest inbound message id per webhook chat — what a bare "react to that"
//! names.
//!
//! Process-global rather than adapter state because webhook adapters are
//! deliberately stateless: `WebhookPlatformDelivery::from_env` reconstructs them
//! per delivery, so anything remembered inside one is gone by the time the tool
//! runs. The two ends live in different places — the ingress route writes, the
//! delivery sink reads — so the map is what joins them.
//
// ponytail: bounded LRU-by-insertion, last id only. Ceiling: reacting to an
// ARBITRARY older message needs the id plumbed through `MessageEvent`. Do that
// when replies and edits want the same plumbing — all three want it together.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Chats remembered before the oldest entries are dropped. A gateway serving
/// several busy platforms should not grow an unbounded map for a feature whose
/// whole point is the message that just arrived.
const MAX_CHATS: usize = 512;

type Seen = Mutex<(HashMap<String, String>, Vec<String>)>;

fn seen() -> &'static Seen {
    static SEEN: OnceLock<Seen> = OnceLock::new();
    SEEN.get_or_init(|| Mutex::new((HashMap::new(), Vec::new())))
}

/// Records `message_id` as the newest message in `platform:chat_id`.
pub(crate) fn remember(platform: &str, chat_id: &str, message_id: &str) {
    let key = format!("{platform}:{chat_id}");
    let mut guard = seen().lock().expect("last_inbound poisoned");
    let (map, order) = &mut *guard;
    if map.insert(key.clone(), message_id.to_owned()).is_none() {
        order.push(key);
    }
    // Evict oldest-first. A chat that keeps talking is re-inserted, not
    // re-queued, so an active conversation can still age out — acceptable for
    // a hint whose miss is a clear error message, not a wrong reaction.
    while order.len() > MAX_CHATS {
        let oldest = order.remove(0);
        map.remove(&oldest);
    }
}

/// The newest message id seen in `platform:chat_id`, if any.
pub(crate) fn latest(platform: &str, chat_id: &str) -> Option<String> {
    seen()
        .lock()
        .expect("last_inbound poisoned")
        .0
        .get(&format!("{platform}:{chat_id}"))
        .cloned()
}

#[cfg(test)]
#[path = "last_inbound_tests.rs"]
mod tests;
