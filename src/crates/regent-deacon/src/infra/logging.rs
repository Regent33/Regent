//! Logging setup (P7 observability): structured logs to stderr (the JSON-RPC
//! stream owns stdout) **and** a daily-rolling file under `$REGENT_HOME/logs/`,
//! with the file writer wrapped so secrets are redacted before they hit disk.

use regent_kernel::RedactingWriter;
use std::path::Path;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
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
/// `regent.log.<date>` under `logs_dir`. If that file cannot be created, Regent
/// keeps running with stderr logging. Hold the optional guard for the process
/// lifetime so buffered file lines are flushed on drop.
pub fn init_logging(logs_dir: &Path) -> Option<WorkerGuard> {
    let _ = std::fs::create_dir_all(logs_dir);
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("regent.log")
        .build(logs_dir);

    match appender {
        Ok(appender) => {
            let (file_writer, guard) = tracing_appender::non_blocking(appender);
            tracing_subscriber::registry()
                .with(stderr_layer())
                .with(
                    fmt::layer()
                        .with_ansi(false)
                        .with_timer(local_timer())
                        .with_writer(RedactingMake { inner: file_writer })
                        .with_filter(log_filter()),
                )
                .init();
            Some(guard)
        }
        Err(error) => {
            tracing_subscriber::registry().with(stderr_layer()).init();
            tracing::warn!(%error, "file logging unavailable; continuing with stderr");
            None
        }
    }
}

fn log_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
}

fn local_timer() -> fmt::time::ChronoLocal {
    fmt::time::ChronoLocal::rfc_3339()
}

fn stderr_layer<S>() -> impl tracing_subscriber::Layer<S>
where
    S: tracing::Subscriber,
    for<'a> S: tracing_subscriber::registry::LookupSpan<'a>,
{
    fmt::layer()
        .with_timer(local_timer())
        .with_writer(std::io::stderr)
        .with_filter(log_filter())
}
