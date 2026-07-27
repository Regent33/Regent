//! Cross-chain provider health and the admission bounds that keep failover from
//! amplifying an outage (W2). Split from `orchestrators.rs` for the file-size
//! rule; the chain walk itself stays there.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
// `tokio::time::Instant`, not `std`: identical in production (it defers to the
// std clock), but it respects a paused test clock, so the minute-scale cooldown
// windows can be asserted without a test that actually sleeps for minutes.
use tokio::time::Instant;

/// How many providers one request may attempt: the first, plus this many
/// failover hops.
///
/// W2 — the amplification bound. Every provider call already retries internally
/// (`run_with_retry`, 3 attempts), so an unbounded walk costs
/// `attempts x chain_length` HTTP calls for ONE turn. On 2026-07-26 that
/// produced 421 rate-limited responses against ~50 provider selections — 81 in
/// a single minute. Failover was amplifying the outage, not mitigating it.
///
/// **This is overload control, not an availability guarantee.** Cooling members
/// are filtered out before the cap applies, so a chain whose bad members are
/// already KNOWN bad still reaches a healthy one. But on a cold chain,
/// `[fail, fail, fail, healthy]` strands the first request: it attempts three,
/// gives up, and only the NEXT request — now armed with cooldown state — reaches
/// the healthy member. That is a deliberate trade of one request's latency for a
/// bound on herd amplification, and it is the honest description of it.
pub(crate) const MAX_FAILOVER_HOPS: usize = 2;

/// Ceiling on a SERVER-STATED `retry-after`, so a hostile or mistaken header
/// cannot park a provider for hours. Deliberately not applied to the locally
/// configured cooldown — capping that would silently override an operator who
/// asked for a longer one.
const MAX_STATED_COOLDOWN: Duration = Duration::from_secs(5 * 60);

/// Cross-chain provider health. A registry shares one instance across all
/// sessions, so opening a new chat does not immediately re-pay the timeout of
/// a provider that just failed. Cooldown expiry provides automatic recovery.
pub struct FallbackHealth {
    cooldown: Duration,
    cooling_until: Mutex<HashMap<String, Instant>>,
}

impl Default for FallbackHealth {
    fn default() -> Self {
        Self::new(Duration::from_secs(30))
    }
}

impl FallbackHealth {
    #[must_use]
    pub fn new(cooldown: Duration) -> Self {
        Self {
            cooldown,
            cooling_until: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn cooling(&self, key: &str) -> bool {
        self.cooling_until
            .lock()
            .unwrap()
            .get(key)
            .is_some_and(|until| *until > Instant::now())
    }

    /// When this key stops cooling, if it is.
    pub(crate) fn cooling_until(&self, key: &str) -> Option<Instant> {
        self.cooling_until
            .lock()
            .unwrap()
            .get(key)
            .copied()
            .filter(|until| *until > Instant::now())
    }

    pub(crate) fn failed(&self, key: &str) {
        self.failed_for(key, None);
    }

    /// Cools a provider, preferring the server's own `retry-after` over our
    /// flat guess. A 429 that says "wait 90s" used to be cooled for 30s, so the
    /// chain came back while the provider was still refusing — one of the ways
    /// a single outage turned into repeated hits.
    pub(crate) fn failed_for(&self, key: &str, retry_after: Option<Duration>) {
        let wait = retry_after.map_or(self.cooldown, |stated| stated.min(MAX_STATED_COOLDOWN));
        self.cooling_until
            .lock()
            .unwrap()
            .insert(key.to_owned(), Instant::now() + wait);
    }

    pub(crate) fn healthy(&self, key: &str) {
        self.cooling_until.lock().unwrap().remove(key);
    }
}
