use crate::domain::errors::StoreError;
use crate::infra::schema::{RECONCILE_COLUMNS, SCHEMA_SQL, SCHEMA_VERSION};
use rand::RngExt;
use rusqlite::{Connection, TransactionBehavior};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

// Hermes write-contention policy, ported: short busy timeout, jittered
// application-level retries (avoids SQLite's deterministic-backoff convoy),
// BEGIN IMMEDIATE so lock contention surfaces at transaction start.
const BUSY_TIMEOUT_MS: u64 = 1_000;
const WRITE_MAX_RETRIES: u32 = 15;
const WRITE_RETRY_MIN_MS: u64 = 20;
const WRITE_RETRY_MAX_MS: u64 = 150;

pub struct Store {
    pub(crate) conn: Mutex<Connection>,
}

impl Store {
    /// Opens (or creates) the database, applies pragmas, and initializes
    /// the schema. The parent directory must exist.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        let store = Self::init(conn)?;
        // Migrate any legacy plaintext persona files (soul.md/about-you.md next
        // to the db) into the DB, then delete them — persona is DB-only now.
        if let Some(home) = path.parent().and_then(Path::to_str) {
            store.import_persona_files(home);
        }
        Ok(store)
    }

    /// In-memory store for tests.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self, StoreError> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(Duration::from_millis(BUSY_TIMEOUT_MS))?;
        conn.execute_batch(SCHEMA_SQL)?;
        reconcile_columns(&conn)?;
        let version: Option<i64> = conn
            .query_row("SELECT version FROM schema_version LIMIT 1", [], |r| r.get(0))
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        match version {
            None => {
                conn.execute("INSERT INTO schema_version (version) VALUES (?1)", [SCHEMA_VERSION])?;
            }
            Some(v) if v == SCHEMA_VERSION => {}
            Some(v) if v < SCHEMA_VERSION => {
                // v1 → v2 is purely additive (reconcile + IF NOT EXISTS above),
                // so reaching here just stamps the new version.
                conn.execute("UPDATE schema_version SET version = ?1", [SCHEMA_VERSION])?;
            }
            Some(v) => {
                tracing::warn!(found = v, expected = SCHEMA_VERSION, "database is newer than this build");
            }
        }
        let store = Self { conn: Mutex::new(conn) };
        store.seed_persona()?; // soul/about rows always exist (DB-backed persona)
        Ok(store)
    }

    /// Runs `f` inside a `BEGIN IMMEDIATE` transaction, retrying on
    /// busy/locked with random jitter.
    pub(crate) fn with_write<T>(
        &self,
        f: impl Fn(&rusqlite::Transaction<'_>) -> Result<T, rusqlite::Error>,
    ) -> Result<T, StoreError> {
        for attempt in 1..=WRITE_MAX_RETRIES {
            let mut guard = self.conn.lock().expect("store mutex poisoned");
            let result = guard
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .and_then(|tx| {
                    let value = f(&tx)?;
                    tx.commit()?;
                    Ok(value)
                });
            drop(guard);
            match result {
                Ok(value) => return Ok(value),
                Err(error) if is_busy(&error) && attempt < WRITE_MAX_RETRIES => {
                    let jitter_ms =
                        rand::rng().random_range(WRITE_RETRY_MIN_MS..=WRITE_RETRY_MAX_MS);
                    std::thread::sleep(Duration::from_millis(jitter_ms));
                }
                Err(error) if is_busy(&error) => {
                    return Err(StoreError::Contention { attempts: attempt });
                }
                Err(error) => return Err(error.into()),
            }
        }
        Err(StoreError::Contention { attempts: WRITE_MAX_RETRIES })
    }

    pub(crate) fn with_read<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, rusqlite::Error>,
    ) -> Result<T, StoreError> {
        let guard = self.conn.lock().expect("store mutex poisoned");
        Ok(f(&guard)?)
    }
}

/// Adds any column listed in `RECONCILE_COLUMNS` that the live table lacks.
/// Idempotent — runs on every open.
fn reconcile_columns(conn: &Connection) -> Result<(), StoreError> {
    for (table, column, declaration) in RECONCILE_COLUMNS {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let existing: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<_, _>>()?;
        if !existing.iter().any(|name| name == column) {
            conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {declaration}"))?;
            tracing::info!(table, column, "reconciled missing column");
        }
    }
    Ok(())
}

fn is_busy(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(e, _)
            if e.code == rusqlite::ErrorCode::DatabaseBusy
                || e.code == rusqlite::ErrorCode::DatabaseLocked
    )
}

/// Unix epoch seconds as float (the timestamp convention of the store).
pub fn now_epoch() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

