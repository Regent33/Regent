//! Bounds how many turns can be pending at once for one conversation. Without
//! this, a burst of platform messages for the same `{platform}:{chat_id}` key
//! (a busy group chat) each spawn a task that blocks on `SessionManager`'s
//! per-agent mutex — unbounded, with no feedback while waiting (the mutex is
//! fair/FIFO, so ordering is fine; there is simply no ceiling and no reply).
//! In-memory and per-process, same acceptable-for-v1 shape as `RateLimiter`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub struct QueueGate {
    max_pending: usize,
    pending: Mutex<HashMap<String, usize>>,
}

/// Held for the lifetime of one admitted turn; dropping it (success, error,
/// or panic-unwind) always releases the slot — a leaked slot would eventually
/// wedge that conversation shut. Owns an `Arc<QueueGate>` clone (not a
/// borrow) so it is `'static` and safe to move into a spawned task alongside
/// the turn it guards.
pub struct QueueGateGuard {
    gate: Arc<QueueGate>,
    key: String,
}

impl QueueGate {
    /// Allow at most `max_pending` turns in flight at once per conversation
    /// key. `0` disables the gate (always admits).
    #[must_use]
    pub fn new(max_pending: usize) -> Self {
        Self {
            max_pending,
            pending: Mutex::new(HashMap::new()),
        }
    }

    /// Reads `REGENT_MAX_PENDING_PER_CHAT`; unset/invalid → 3 (a live turn
    /// plus a couple of queued follow-ups — generous for a normal
    /// back-and-forth, still a real ceiling for a flooding burst). `0` disables.
    #[must_use]
    pub fn from_env() -> Self {
        let max_pending = std::env::var("REGENT_MAX_PENDING_PER_CHAT")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(3);
        Self::new(max_pending)
    }

    /// Try to admit one more pending turn for `key`. `None` means the
    /// conversation is already at capacity — the caller should reply with a
    /// bounded "still working" message instead of running (or queueing) the
    /// turn. The returned guard owns an `Arc` clone (not a borrow), so it can
    /// be moved into a spawned task for the turn's whole lifetime and still
    /// release the slot when dropped, wherever that happens.
    pub fn try_enter(self: &Arc<Self>, key: &str) -> Option<QueueGateGuard> {
        if self.max_pending == 0 {
            return Some(QueueGateGuard {
                gate: Arc::clone(self),
                key: String::new(),
            });
        }
        let mut pending = self.pending.lock().expect("queue-gate mutex poisoned");
        let count = pending.entry(key.to_owned()).or_insert(0);
        if *count >= self.max_pending {
            return None;
        }
        *count += 1;
        Some(QueueGateGuard {
            gate: Arc::clone(self),
            key: key.to_owned(),
        })
    }

    #[cfg(test)]
    fn pending_count(&self, key: &str) -> usize {
        self.pending.lock().unwrap().get(key).copied().unwrap_or(0)
    }
}

impl Drop for QueueGateGuard {
    fn drop(&mut self) {
        if self.key.is_empty() {
            return; // the disabled-gate sentinel guard — nothing to release
        }
        let mut pending = self.gate.pending.lock().expect("queue-gate mutex poisoned");
        if let Some(count) = pending.get_mut(&self.key) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                pending.remove(&self.key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admits_up_to_capacity_then_refuses() {
        let gate = Arc::new(QueueGate::new(2));
        let a = gate.try_enter("slack:room1");
        let b = gate.try_enter("slack:room1");
        assert!(a.is_some() && b.is_some());
        assert!(
            gate.try_enter("slack:room1").is_none(),
            "3rd over the cap of 2 is refused"
        );
        // A different conversation has its own independent ceiling.
        assert!(gate.try_enter("slack:room2").is_some());
    }

    #[test]
    fn dropping_a_guard_frees_the_slot_for_the_next_message() {
        let gate = Arc::new(QueueGate::new(1));
        let first = gate.try_enter("tg:chat1").expect("first admitted");
        assert!(
            gate.try_enter("tg:chat1").is_none(),
            "at capacity while first is held"
        );
        drop(first);
        assert!(
            gate.try_enter("tg:chat1").is_some(),
            "slot freed once the guard drops"
        );
    }

    #[test]
    fn zero_disables_the_gate_and_never_refuses() {
        let gate = Arc::new(QueueGate::new(0));
        let mut guards = Vec::new();
        for _ in 0..1000 {
            guards.push(
                gate.try_enter("any:chat")
                    .expect("disabled gate always admits"),
            );
        }
    }

    #[test]
    fn releasing_the_last_guard_removes_the_key_entirely() {
        let gate = Arc::new(QueueGate::new(5));
        let guard = gate.try_enter("wa:c1").unwrap();
        assert_eq!(gate.pending_count("wa:c1"), 1);
        drop(guard);
        assert_eq!(
            gate.pending_count("wa:c1"),
            0,
            "no leaked zero-count entries"
        );
    }
}
