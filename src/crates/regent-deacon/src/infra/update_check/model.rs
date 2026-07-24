//! Compact release-manifest model + strict `x.y.z` semantic-version compare.
//!
//! Only the fields Phase 0 (notify-only) needs are typed. Unknown fields and
//! unknown components are TOLERATED — serde ignores them — so a newer manifest
//! never breaks an older client. Remote asset names/URLs are intentionally NOT
//! read or applied here; a client derives the official release URL itself.

use serde::Deserialize;
use std::collections::HashMap;

/// Hard cap on a manifest body. The real manifest is a few KiB; anything far
/// larger is treated as malformed and rejected before it is parsed.
pub const MAX_MANIFEST_BYTES: usize = 128 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("manifest too large: {0} bytes")]
    TooLarge(usize),
    #[error("manifest is not valid JSON: {0}")]
    Json(String),
    #[error("unsupported manifest schema: {0}")]
    UnsupportedSchema(u32),
}

/// A strict `major.minor.patch` version. A single leading `v` is tolerated;
/// everything else must be exactly three dot-separated integers. Derived `Ord`
/// gives correct numeric precedence (0.1.10 > 0.1.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    /// Parse `x.y.z` (optionally `vx.y.z`). Returns `None` for anything that is
    /// not exactly three numeric components — no pre-release/build handling in
    /// Phase 0 (the release channel ships plain `x.y.z`).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let s = s.strip_prefix('v').unwrap_or(s);
        let mut it = s.split('.');
        let major = it.next()?.parse().ok()?;
        let minor = it.next()?.parse().ok()?;
        let patch = it.next()?.parse().ok()?;
        if it.next().is_some() {
            return None; // reject x.y.z.w and trailing junk
        }
        Some(Self {
            major,
            minor,
            patch,
        })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// `latest` is a strictly newer release than `current`. A missing/unparseable
/// side means "no upgrade offered" (fail safe — never nag on garbage).
#[must_use]
pub fn is_newer(current: &str, latest: Option<&str>) -> bool {
    match (Version::parse(current), latest.and_then(Version::parse)) {
        (Some(c), Some(l)) => l > c,
        _ => false,
    }
}

/// The whole manifest. Only `schema` and `channels` are read; every other
/// top-level key (`generated_at`, `signing_key_id`, …) is ignored on purpose.
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub schema: u32,
    #[serde(default)]
    pub channels: HashMap<String, Channel>,
}

/// One release channel. Phase 0 needs only the version string; protocols,
/// components, rollback, and assets are ignored here (added in later phases).
#[derive(Debug, Clone, Deserialize)]
pub struct Channel {
    pub version: String,
}

impl Manifest {
    /// The stable channel's parsed version, if present and well-formed.
    #[must_use]
    pub fn stable_version(&self) -> Option<Version> {
        self.channels
            .get("stable")
            .and_then(|c| Version::parse(&c.version))
    }
}

/// Size-checked JSON parse. Oversized input is rejected before parsing; invalid
/// JSON is reported without leaking the (potentially huge) body.
pub fn parse_manifest(bytes: &[u8]) -> Result<Manifest, ManifestError> {
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(ManifestError::TooLarge(bytes.len()));
    }
    let manifest: Manifest =
        serde_json::from_slice(bytes).map_err(|error| ManifestError::Json(error.to_string()))?;
    if manifest.schema != 1 {
        return Err(ManifestError::UnsupportedSchema(manifest.schema));
    }
    Ok(manifest)
}
