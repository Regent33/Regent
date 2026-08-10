//! Cross-process ownership for background learning reviews.
//!
//! Every transition runs under the store's `BEGIN IMMEDIATE` writer, so two
//! deacons sharing one Regent home cannot both acquire the same parent range.

use crate::domain::entities::{ReviewClaimOutcome, ReviewLease};
use crate::domain::errors::StoreError;
use crate::infra::db::{Store, now_epoch};
use regent_kernel::SessionId;
use rusqlite::{OptionalExtension, params};

impl Store {
    /// Atomically claims `[reviewed_message_count, target)` for one process.
    pub fn claim_session_review_range(
        &self,
        id: &SessionId,
        target: usize,
        lease_secs: f64,
    ) -> Result<ReviewClaimOutcome, StoreError> {
        let target_i64 = i64::try_from(target).unwrap_or(i64::MAX);
        let now = now_epoch();
        let ttl = if lease_secs.is_finite() {
            lease_secs.max(0.0)
        } else {
            0.0
        };
        let token = format!("review-{}", SessionId::generate());
        let outcome = self.with_write(|tx| {
            let row: Option<(i64, Option<String>, Option<f64>)> = tx
                .query_row(
                    "SELECT reviewed_message_count, review_claim_token, review_claim_until
                     FROM sessions WHERE id = ?1",
                    params![id.as_str()],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            let Some((reviewed, current_token, current_until)) = row else {
                return Ok(None);
            };
            let reviewed = reviewed.max(0);
            if reviewed >= target_i64 {
                return Ok(Some(ReviewClaimOutcome::Covered {
                    reviewed_message_count: usize::try_from(reviewed).unwrap_or(usize::MAX),
                }));
            }
            if current_token.is_some() && current_until.is_some_and(|until| until > now) {
                return Ok(Some(ReviewClaimOutcome::Busy));
            }
            tx.execute(
                "UPDATE sessions
                 SET review_claim_token = ?1, review_claim_start = ?2,
                     review_claim_end = ?3, review_claim_until = ?4
                 WHERE id = ?5",
                params![token, reviewed, target_i64, now + ttl, id.as_str()],
            )?;
            Ok(Some(ReviewClaimOutcome::Acquired(ReviewLease {
                token: token.clone(),
                start: usize::try_from(reviewed).unwrap_or(usize::MAX),
                end: target,
            })))
        })?;
        outcome.ok_or_else(|| StoreError::UnknownSession(id.to_string()))
    }

    /// Extends a live lease. A stale token cannot renew a successor's claim.
    pub fn renew_session_review_claim(
        &self,
        id: &SessionId,
        token: &str,
        lease_secs: f64,
    ) -> Result<bool, StoreError> {
        let ttl = if lease_secs.is_finite() {
            lease_secs.max(0.0)
        } else {
            0.0
        };
        self.with_write(|tx| {
            let changed = tx.execute(
                "UPDATE sessions SET review_claim_until = ?1
                 WHERE id = ?2 AND review_claim_token = ?3",
                params![now_epoch() + ttl, id.as_str(), token],
            )?;
            Ok(changed == 1)
        })
    }

    /// Commits the claimed end cursor and clears ownership in one transaction.
    pub fn finish_session_review_claim(
        &self,
        id: &SessionId,
        token: &str,
    ) -> Result<bool, StoreError> {
        self.with_write(|tx| {
            let changed = tx.execute(
                "UPDATE sessions
                 SET reviewed_message_count = MAX(reviewed_message_count, review_claim_end),
                     review_claim_token = NULL, review_claim_start = NULL,
                     review_claim_end = NULL, review_claim_until = NULL
                 WHERE id = ?1 AND review_claim_token = ?2",
                params![id.as_str(), token],
            )?;
            Ok(changed == 1)
        })
    }

    /// Known failure: release now. A crash cannot call this and waits for TTL.
    pub fn release_session_review_claim(
        &self,
        id: &SessionId,
        token: &str,
    ) -> Result<bool, StoreError> {
        self.with_write(|tx| {
            let changed = tx.execute(
                "UPDATE sessions
                 SET review_claim_token = NULL, review_claim_start = NULL,
                     review_claim_end = NULL, review_claim_until = NULL
                 WHERE id = ?1 AND review_claim_token = ?2",
                params![id.as_str(), token],
            )?;
            Ok(changed == 1)
        })
    }
}
