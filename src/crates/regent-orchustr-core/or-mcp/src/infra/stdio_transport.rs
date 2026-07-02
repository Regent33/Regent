use crate::domain::contracts::McpTransport;
use crate::domain::entities::JsonRpcMessage;
use crate::domain::errors::McpError;
use crate::infra::jsonrpc::{decode_message, encode_message};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

type SharedLines = Arc<Mutex<Lines<BufReader<ChildStdout>>>>;

#[derive(Debug, Clone)]
pub struct StdioTransport {
    _child: Arc<Mutex<Child>>,
    reader: SharedLines,
    writer: Arc<Mutex<ChildStdin>>,
}

impl StdioTransport {
    pub fn spawn(command: &str, args: &[&str]) -> Result<Self, McpError> {
        let mut child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|error| McpError::Transport(error.to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::Transport("child stdout was not piped".to_owned()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| McpError::Transport("child stdin was not piped".to_owned()))?;
        Ok(Self {
            _child: Arc::new(Mutex::new(child)),
            reader: Arc::new(Mutex::new(BufReader::new(stdout).lines())),
            writer: Arc::new(Mutex::new(stdin)),
        })
    }
}

impl McpTransport for StdioTransport {
    async fn send_message(&self, msg: &JsonRpcMessage) -> Result<(), McpError> {
        let payload = format!("{}\n", encode_message(msg)?);
        let mut writer = self.writer.lock().await;
        writer
            .write_all(payload.as_bytes())
            .await
            .map_err(|error| McpError::Transport(error.to_string()))?;
        writer
            .flush()
            .await
            .map_err(|error| McpError::Transport(error.to_string()))
    }

    async fn receive_message(&self) -> Result<JsonRpcMessage, McpError> {
        let mut reader = self.reader.lock().await;
        let line = reader
            .next_line()
            .await
            .map_err(|error| McpError::Transport(error.to_string()))?;
        let line = line.ok_or_else(|| {
            McpError::Transport(
                "stdio stream closed before a JSON-RPC message was received".to_owned(),
            )
        })?;
        decode_message(&line)
    }
}
