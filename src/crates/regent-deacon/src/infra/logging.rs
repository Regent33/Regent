//! Logging setup (P7 observability): structured logs to stderr (the JSON-RPC
//! stream owns stdout) **and** a daily-rolling file under `$REGENT_HOME/logs/`,
//! with the file writer wrapped so secrets are redacted before they hit disk.

use regent_kernel::RedactingWriter;
use std::path::Path;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::{self, MakeWriter};
use tracing_subscriber::prelude::*;

/// Wraps an inner `MakeWriter` so every produced writer redacts secrets.
struct RedactingMake<M> {
    inner: M,
}

impl<'a, M: MakeWriter<'a>> MakeWriter<'a> for RedactingMake<M> {
    type Writer = RedactingWriter<M::Writer>;
    fn make_writer(&'a self) -> Self::Writer {
        RedactingWriter::new(self.inner.make_writer())
    }
}

/// Initializes tracing: human logs to stderr plus a redacted, daily-rolling
/// `regent.log.<date>` under `logs_dir`. Returns the appender guard — the
/// caller must hold it for the process lifetime (drop flushes buffered lines).
pub fn init_logging(logs_dir: &Path) -> WorkerGuard {
    let _ = std::fs::create_dir_all(logs_dir);
    let (file_writer, guard) =
        tracing_appender::non_blocking(tracing_appender::rolling::daily(logs_dir, "regent.log"));

    // Default to `info` when RUST_LOG is unset — an empty EnvFilter logs
    // NOTHING, which left failovers/errors invisible in the log file.
    let filter = || {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };
    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_writer(std::io::stderr)
                .with_filter(filter()),
        )
        .with(
            fmt::layer()
                .with_ansi(false)
                .with_writer(RedactingMake { inner: file_writer })
                .with_filter(filter()),
        )
        .init();
    guard
}
