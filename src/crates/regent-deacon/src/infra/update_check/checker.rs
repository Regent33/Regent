use super::cache::CacheFile;
use super::fetch::{FetchOutcome, conditional_get};
use super::lock::CheckLock;
use super::model::{is_newer, parse_manifest};
use super::{
    HTTP_TIMEOUT_SECS, JITTER_WINDOW_SECS, OFFICIAL_REPO, TTL_SECS, allowed_manifest_host,
    manifest_url,
};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize)]
pub struct UpdateStatus {
    pub current: String,
    pub latest: Option<String>,
    pub available: bool,
    pub checked_at: Option<i64>,
    pub source: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

pub struct UpdateChecker {
    home: PathBuf,
    current: String,
    status: RwLock<UpdateStatus>,
}

impl UpdateChecker {
    #[must_use]
    pub fn new(home: PathBuf, current: String) -> Self {
        let status = status_from_cache(&current, CacheFile::load(&home));
        Self {
            home,
            current,
            status: RwLock::new(status),
        }
    }

    #[must_use]
    pub fn status(&self) -> UpdateStatus {
        self.status.read().unwrap().clone()
    }

    pub fn spawn(self: Arc<Self>) {
        tokio::spawn(async move { self.run().await });
    }

    async fn run(&self) {
        if is_optout(std::env::var("REGENT_NO_UPDATE_CHECK").ok().as_deref()) {
            *self.status.write().unwrap() = UpdateStatus {
                current: self.current.clone(),
                latest: None,
                available: false,
                checked_at: None,
                source: "disabled",
                note: None,
            };
            return;
        }
        loop {
            let checked_at = CacheFile::load(&self.home).map_or(0, |c| c.checked_at);
            let wait = due_at(checked_at, self.jitter()) - now_secs();
            if wait <= 0 {
                if !self.check_once().await {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                }
                continue;
            }
            tokio::time::sleep(Duration::from_secs(wait as u64)).await;
        }
    }

    async fn check_once(&self) -> bool {
        let Some(_lock) = CheckLock::acquire(&self.home) else {
            return false;
        };
        let cache = CacheFile::load(&self.home).unwrap_or_default();
        if cache.checked_at != 0 && now_secs() < due_at(cache.checked_at, self.jitter()) {
            self.commit(cache, "cache");
            return true;
        }
        let client = match reqwest::Client::builder()
            .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
            .user_agent(concat!("regent-deacon/", env!("CARGO_PKG_VERSION")))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if allowed_manifest_host(attempt.url()) {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .build()
        {
            Ok(client) => client,
            Err(error) => {
                self.fail(cache, &error.to_string());
                return true;
            }
        };
        let url = manifest_url(OFFICIAL_REPO);
        match conditional_get(&client, &url, cache.etag.as_deref()).await {
            FetchOutcome::NotModified => self.commit(
                CacheFile {
                    checked_at: now_secs(),
                    diagnostic: None,
                    ..cache
                },
                "network",
            ),
            FetchOutcome::Fetched { body, etag } => match parse_manifest(&body) {
                Ok(manifest) => match manifest.stable_version() {
                    Some(version) => self.commit(
                        CacheFile {
                            etag,
                            checked_at: now_secs(),
                            latest: Some(version.to_string()),
                            diagnostic: None,
                        },
                        "network",
                    ),
                    None => self.fail(cache, "manifest has no stable channel version"),
                },
                Err(error) => self.fail(cache, &error.to_string()),
            },
            FetchOutcome::Failed(reason) => self.fail(cache, &reason),
        }
        true
    }

    fn commit(&self, cache: CacheFile, source: &'static str) {
        let _ = cache.store(&self.home);
        let mut status = status_from_cache(&self.current, Some(cache));
        status.source = source;
        *self.status.write().unwrap() = status;
    }

    fn fail(&self, cache: CacheFile, reason: &str) {
        self.commit(
            CacheFile {
                checked_at: now_secs(),
                diagnostic: Some(super::cache::bound(reason)),
                ..cache
            },
            "cache",
        );
    }

    pub(super) fn jitter(&self) -> i64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in self.home.to_string_lossy().bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        (hash % JITTER_WINDOW_SECS as u64) as i64
    }
}

pub(super) fn status_from_cache(current: &str, cache: Option<CacheFile>) -> UpdateStatus {
    match cache {
        Some(cache) => UpdateStatus {
            current: current.to_owned(),
            available: is_newer(current, cache.latest.as_deref()),
            latest: cache.latest,
            checked_at: (cache.checked_at != 0).then_some(cache.checked_at),
            source: "cache",
            note: cache.diagnostic,
        },
        None => UpdateStatus {
            current: current.to_owned(),
            latest: None,
            available: false,
            checked_at: None,
            source: "never",
            note: None,
        },
    }
}

fn due_at(checked_at: i64, jitter: i64) -> i64 {
    if checked_at == 0 {
        0
    } else {
        checked_at + TTL_SECS + jitter
    }
}

pub(super) fn is_optout(value: Option<&str>) -> bool {
    matches!(value.map(str::trim), Some("1" | "true" | "yes"))
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}
