//! Stdio JSON-RPC transport: line-delimited reads from stdin; writes via a
//! channel drained by a dedicated write task (avoids locking stdout).

use crate::domain::contracts::OutboundTx;
use tokio::io::{AsyncBufReadExt, BufReader};

pub struct StdioTransport {
    reader: BufReader<tokio::io::Stdin>,
}

impl StdioTransport {
    #[must_use]
    pub fn new() -> Self {
        Self {
            reader: BufReader::new(tokio::io::stdin()),
        }
    }

    /// Returns the next non-empty line, or None on EOF/read error.
    /// Blank lines are skipped — they must never read as end-of-stream.
    pub async fn next_line(&mut self) -> Option<String> {
        loop {
            let mut line = String::new();
            match self.reader.read_line(&mut line).await {
                Ok(0) | Err(_) => return None,
                Ok(_) => {
                    let trimmed = line.trim_end_matches(['\n', '\r']);
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_owned());
                    }
                }
            }
        }
    }
}

impl Default for StdioTransport {
    fn default() -> Self {
        Self::new()
    }
}

/// Spawns a task that drains `rx` and writes each message as a line to stdout.
/// Returns the sender half for sharing with session tasks.
pub fn spawn_write_loop() -> OutboundTx {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        let mut out = tokio::io::stdout();
        while let Some(mut line) = rx.recv().await {
            line.push('\n');
            if out.write_all(line.as_bytes()).await.is_err() {
                break;
            }
            let _ = out.flush().await;
        }
    });
    tx
}
