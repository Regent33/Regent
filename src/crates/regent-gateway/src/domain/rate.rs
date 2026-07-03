//! Per-user inbound rate limiter (W2.4 Layer A). A token bucket keyed by
//! `platform:user_id`, shared across the ingress planes (gateway runner +
//! deacon webhook/Discord) the same way `AuthPolicy` is. In-memory and
//! per-process — a restart resets buckets, and two processes don't share state
//! (acceptable for v1; the cap is a flood brake, not an accounting ledger).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

/// A per-user token bucket. `check(key)` consumes one token and returns whether
/// the request is allowed. A limiter built with `per_minute(0)` is **disabled**
/// (always allows) — the default when no rate is configured.
pub struct RateLimiter {
    /// Max burst (also the steady per-minute allowance).
    capacity: f64,
    /// Tokens refilled per second (`capacity / 60`).
    refill_per_sec: f64,
    buckets: Mutex<HashMap<String, Bucket>>,
}

struct Bucket {
    tokens: f64,
    last: Instant,
}

impl RateLimiter {
    /// Allow `per_min` messages per user per minute, permitting a full-minute
    /// burst. `0` disables the limiter (always allows).
    #[must_use]
    pub fn per_minute(per_min: u32) -> Self {
        Self {
            capacity: f64::from(per_min),
            refill_per_sec: f64::from(per_min) / 60.0,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Reads `REGENT_MESSAGES_PER_MIN` (per user); unset/invalid/`0` → disabled.
    /// The ingress planes build the limiter from this so the gateway and the
    /// deacon webhook share one knob, mirroring the auth env config.
    #[must_use]
    pub fn from_env() -> Self {
        let per_min = std::env::var("REGENT_MESSAGES_PER_MIN")
            .ok()
            .and_then(|v| v.trim().parse::<u32>().ok())
            .unwrap_or(0);
        Self::per_minute(per_min)
    }

    /// Consume one token for `key`. Returns `true` when allowed, `false` when the
    /// user has exhausted their bucket (rate-limited). A disabled limiter (`0`)
    /// always returns `true`.
    pub fn check(&self, key: &str) -> bool {
        if self.capacity <= 0.0 {
            return true; // disabled
        }
        let now = Instant::now();
        let mut buckets = self.buckets.lock().expect("rate mutex poisoned");
        let bucket = buckets.entry(key.to_owned()).or_insert(Bucket {
            tokens: self.capacity,
            last: now,
        });
        let elapsed = now.duration_since(bucket.last).as_secs_f64();
        bucket.last = now;
        bucket.tokens = (bucket.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_up_to_capacity_then_denies() {
        let rl = RateLimiter::per_minute(3); // bucket starts full at 3
        assert!(rl.check("slack:u1"));
        assert!(rl.check("slack:u1"));
        assert!(rl.check("slack:u1"));
        assert!(!rl.check("slack:u1"), "4th within the window is denied");
        // A different user has an independent bucket.
        assert!(rl.check("slack:u2"));
    }

    #[test]
    fn zero_is_disabled_and_never_limits() {
        let rl = RateLimiter::per_minute(0);
        for _ in 0..1000 {
            assert!(rl.check("anyone"));
        }
    }
}
