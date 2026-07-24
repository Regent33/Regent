//! Detached, cached release checker (ADR-041 Phase 0 — notify only).
//!
//! Checks are bounded, fail-silent, and coordinated per Regent home. The
//! checker never applies an update and never reads a download URL from remote
//! data. `REGENT_NO_UPDATE_CHECK=1` disables it.

mod cache;
mod checker;
mod fetch;
mod lock;
mod model;

#[cfg(test)]
#[path = "http_tests.rs"]
mod http_tests;
#[cfg(test)]
#[path = "storage_tests.rs"]
mod storage_tests;
#[cfg(test)]
#[path = "tests.rs"]
mod tests;

pub use checker::{UpdateChecker, UpdateStatus};
pub use model::{Version, is_newer, parse_manifest};

use reqwest::Url;

pub(super) const OFFICIAL_REPO: &str = "Regent33/Regent";
pub(super) const MANIFEST_ASSET: &str = "regent-manifest.json";
pub(super) const TTL_SECS: i64 = 24 * 60 * 60;
pub(super) const JITTER_WINDOW_SECS: i64 = 6 * 60 * 60;
pub(super) const HTTP_TIMEOUT_SECS: u64 = 10;

pub(super) fn manifest_url(repo: &str) -> String {
    format!("https://github.com/{repo}/releases/latest/download/{MANIFEST_ASSET}")
}

/// GitHub release downloads redirect to its asset CDN; no other host is needed.
pub(super) fn allowed_manifest_host(url: &Url) -> bool {
    matches!(
        url.host_str(),
        Some(
            "github.com" | "objects.githubusercontent.com" | "release-assets.githubusercontent.com"
        )
    )
}
