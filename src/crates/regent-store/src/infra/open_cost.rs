//! Where `Store::open` spends its time on an ALREADY-MIGRATED store.
//!
//! Measurement, not a unit test — `--ignored`, like W10's ANN gate. Every
//! number here is steady-state cost paid on *every* deacon boot and every CLI
//! command, since the CLI spawns a deacon per invocation.
//!
//!   cargo test -p regent-store --lib open_cost -- --ignored --nocapture

use super::db::Store;
use super::schema::{RECONCILE_COLUMNS, SCHEMA_SQL};
use super::schema_jobs::JOBS_SCHEMA_SQL;
use rusqlite::Connection;
use std::time::{Duration, Instant};

fn med(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

#[test]
#[ignore = "measurement, not a gate"]
fn open_cost_phase_split() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("probe.db");
    drop(Store::open(&path).unwrap()); // create + migrate once

    let (mut open, mut pragmas, mut schema, mut jobs, mut recon, mut ver) =
        (vec![], vec![], vec![], vec![], vec![], vec![]);

    for _ in 0..21 {
        let t = Instant::now();
        let conn = Connection::open(&path).unwrap();
        open.push(t.elapsed().as_secs_f64() * 1000.0);

        let t = Instant::now();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        conn.pragma_update(None, "synchronous", "NORMAL").unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        conn.busy_timeout(Duration::from_millis(5_000)).unwrap();
        pragmas.push(t.elapsed().as_secs_f64() * 1000.0);

        let t = Instant::now();
        conn.execute_batch(SCHEMA_SQL).unwrap();
        schema.push(t.elapsed().as_secs_f64() * 1000.0);

        let t = Instant::now();
        conn.execute_batch(JOBS_SCHEMA_SQL).unwrap();
        jobs.push(t.elapsed().as_secs_f64() * 1000.0);

        let t = Instant::now();
        for (table, column, _) in RECONCILE_COLUMNS {
            let mut stmt = conn
                .prepare(&format!("PRAGMA table_info({table})"))
                .unwrap();
            let cols: Vec<String> = stmt
                .query_map([], |r| r.get::<_, String>(1))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            std::hint::black_box(cols.iter().any(|c| c == column));
        }
        recon.push(t.elapsed().as_secs_f64() * 1000.0);

        let t = Instant::now();
        let _: i64 = conn
            .query_row("SELECT version FROM schema_version LIMIT 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        ver.push(t.elapsed().as_secs_f64() * 1000.0);
    }

    // The phases above are only the ones inside `init`. `Store::open` also
    // opens a SECOND read-only connection and runs the persona paths, and the
    // first cut of this measurement left them out — the parts measured summed
    // to well under the whole, which is how the omission showed up.
    let (mut read_conn, mut whole) = (vec![], vec![]);
    for _ in 0..21 {
        let t = Instant::now();
        let read = Connection::open_with_flags(
            &path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                | rusqlite::OpenFlags::SQLITE_OPEN_URI
                | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .unwrap();
        read.busy_timeout(Duration::from_millis(5_000)).unwrap();
        drop(read);
        read_conn.push(t.elapsed().as_secs_f64() * 1000.0);

        let t = Instant::now();
        drop(Store::open(&path).unwrap());
        whole.push(t.elapsed().as_secs_f64() * 1000.0);
    }

    // Drop the first rep: it warms the page cache for every phase after it.
    let d = |v: Vec<f64>| med(v[1..].to_vec());
    let (o, p, s, j, r, v) = (d(open), d(pragmas), d(schema), d(jobs), d(recon), d(ver));
    let (rc, w) = (d(read_conn), d(whole));
    println!("\n  Connection::open    {o:7.3} ms");
    println!("  pragmas             {p:7.3} ms   <- WAL/synchronous are writes");
    println!("  SCHEMA_SQL          {s:7.3} ms   <- re-run every open");
    println!("  JOBS_SCHEMA_SQL     {j:7.3} ms   <- re-run every open");
    println!("  reconcile_columns   {r:7.3} ms   <- re-run every open");
    println!("  version query       {v:7.3} ms   <- would gate the three above");
    println!("  2nd (read) conn     {rc:7.3} ms");
    let measured = o + p + s + j + r + v + rc;
    println!("  ------------------------------");
    println!("  measured phases     {measured:7.3} ms");
    println!("  whole Store::open   {w:7.3} ms");
    println!(
        "  UNACCOUNTED         {:7.3} ms   <- persona seed/import, not in the list above\n",
        w - measured
    );
}
