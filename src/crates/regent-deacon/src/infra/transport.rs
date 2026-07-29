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
    ///
    /// A leading byte-order mark is stripped. Plenty of Windows writers emit
    /// one on the first write of a UTF-8 stream — PowerShell 5.1 does it and
    /// gives you no way to turn it off — and `serde_json` then fails at
    /// "line 1 column 1" on a request that is otherwise perfectly valid. Only
    /// the FIRST line of such a stream carries it, so the symptom is a client
    /// whose opening request vanishes and whose second one works, which reads
    /// like anything but an encoding problem. Same class as the BOM that made
    /// `validate_content` refuse writes; this is the transport's copy of it.
    pub async fn next_line(&mut self) -> Option<String> {
        loop {
            let mut line = String::new();
            match self.reader.read_line(&mut line).await {
                Ok(0) | Err(_) => return None,
                Ok(_) => {
                    let trimmed =
                        regent_kernel::strip_bom(line.trim_end_matches(['\n', '\r'])).trim_start();
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

#[cfg(test)]
mod tests {
    /// The transport strips what `serde_json` cannot see past. Asserted on the
    /// same helper the transport uses, because a BOM is invisible in a diff and
    /// a regression here silently eats every client's first request.
    #[test]
    fn a_leading_byte_order_mark_does_not_hide_a_valid_request() {
        let with_bom = "\u{feff}{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"health\"}";
        assert!(
            serde_json::from_str::<serde_json::Value>(with_bom).is_err(),
            "premise: the raw line really is unparseable"
        );
        let cleaned = regent_kernel::strip_bom(with_bom).trim_start();
        let parsed: serde_json::Value =
            serde_json::from_str(cleaned).expect("parses once stripped");
        assert_eq!(parsed["method"], "health");
    }

    #[test]
    fn a_line_without_a_mark_is_untouched() {
        let plain = "{\"jsonrpc\":\"2.0\"}";
        assert_eq!(regent_kernel::strip_bom(plain).trim_start(), plain);
    }
}
