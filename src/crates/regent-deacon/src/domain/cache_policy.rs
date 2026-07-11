//! SPL P2 cadence gate (`docs/proposal/token-efficiency-architecture-v1.md`
//! §3.2). Explicit Anthropic `cache_control` breakpoints pay only when a session
//! chains a second turn inside the cache TTL — a write costs 1.25×/2× and never
//! collects a read otherwise. The cadence study
//! (`docs/audits/2026-07-10-cadence-study.md`) measured this per session SOURCE
//! against 950 sessions of ground truth and produced the verdicts encoded below.
//!
//! This is deliberately code-level policy, not config: the study is the source
//! of truth, and coding the surface exclusions explicitly (rather than relying
//! on a runtime expected-reads gate) avoids paying for a detection attempt on a
//! surface already known never to pay (`review`: 660/660 single-turn).

use regent_providers::{CachePolicy, CacheTtl};

/// The prompt-cache policy for a session's source, or `None` for no breakpoints.
///
/// Verdicts (cadence study §Verdict per surface):
/// - `deacon`, `daemon` → **5m**: tight internal call loops (deacon median gap
///   8s, 96.6% within 5m); 1h buys almost nothing for 2× the write premium.
/// - `telegram` → **1h**: human-paced chat (median gap 58s, p90 4m); 100% of
///   gaps land inside 1h vs. 92.3% inside 5m — the only surface where 1h wins.
/// - `review`, `delegate` → **none**: hard exclusion — every review session is
///   single-turn (0 of 660 ever reach a second turn), so a write can never be
///   read. `delegate` has one single-turn session on record → same shape.
/// - anything else → **none**: conservative default for an unknown source.
#[must_use]
pub fn cache_policy_for_source(source: &str) -> Option<CachePolicy> {
    match source {
        "deacon" | "daemon" => Some(CachePolicy {
            ttl: CacheTtl::FiveMinutes,
        }),
        "telegram" => Some(CachePolicy {
            ttl: CacheTtl::OneHour,
        }),
        // review/delegate are single-turn; unknown sources are treated the same.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Deliverable 5(d): the cadence-gate verdicts, per the study.
    #[test]
    fn internal_loops_get_five_minute_breakpoints() {
        for source in ["deacon", "daemon"] {
            assert_eq!(
                cache_policy_for_source(source),
                Some(CachePolicy {
                    ttl: CacheTtl::FiveMinutes
                }),
                "{source} should get 5m breakpoints"
            );
        }
    }

    #[test]
    fn telegram_gets_one_hour_breakpoints() {
        assert_eq!(
            cache_policy_for_source("telegram"),
            Some(CachePolicy {
                ttl: CacheTtl::OneHour
            })
        );
    }

    #[test]
    fn review_and_delegate_are_hard_excluded() {
        assert_eq!(cache_policy_for_source("review"), None);
        assert_eq!(cache_policy_for_source("delegate"), None);
    }

    #[test]
    fn unknown_source_is_conservative_none() {
        assert_eq!(cache_policy_for_source("cron"), None);
        assert_eq!(cache_policy_for_source("kanban"), None);
        assert_eq!(cache_policy_for_source(""), None);
    }
}
