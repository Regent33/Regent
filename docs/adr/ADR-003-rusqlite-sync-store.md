# ADR-003: rusqlite (bundled) with a sync API bridged once via spawn_blocking

**Status:** Accepted

**Context:** The store needs WAL mode, FTS5, `BEGIN IMMEDIATE`, and Hermes's jittered
application-level write-retry policy. Options: sqlx (async, used by or-recall's sqlite feature) or
rusqlite (sync, bundled SQLite with FTS5 guaranteed, direct transaction-behavior control).

**Decision:** `rusqlite` with the `bundled` feature. The store API is synchronous;
`regent-agent` bridges it off the Tokio runtime in exactly one place (`Agent::persist`).

**Consequences:** Full control over pragmas, transaction behavior, and busy-retry semantics;
FTS5 always present regardless of system SQLite. Async ergonomics cost is contained to one
`spawn_blocking` seam. If contention profiling ever demands a pool, the API surface (not callers)
changes.
