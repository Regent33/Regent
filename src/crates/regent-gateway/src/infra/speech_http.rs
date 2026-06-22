//! reqwest-backed [`HttpExecutor`] for the gateway's voice stack. `regent-speech`
//! builds requests purely and stays network-free; this is the gateway's one
//! place that touches the speech wire (the daemon has its own copy — neither
//! binary can depend on the other).
//!
//! The kernel speech traits are sync, so `execute` blocks. The Telegram adapter
//! only calls transcribe/synthesize from a `spawn_blocking` task, so blocking on
//! the stored runtime `Handle` here is safe — never call it from a worker thread.

use regent_speech::{HttpBody, HttpExecutor, SpeechHttpRequest};
use tokio::runtime::Handle;

pub struct ReqwestExecutor {
    client: reqwest::Client,
    handle: Handle,
}

impl ReqwestExecutor {
    /// Capture the current runtime handle. Must be constructed inside the tokio
    /// runtime (the gateway composition root is async).
    #[allow(clippy::new_without_default)]
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            handle: Handle::current(),
        }
    }
}

impl HttpExecutor for ReqwestExecutor {
    fn execute(&self, request: SpeechHttpRequest) -> Result<Vec<u8>, String> {
        self.handle.block_on(async move {
            let mut builder = self.client.post(&request.url);
            if !request.api_key.is_empty() {
                builder = builder.bearer_auth(&request.api_key);
            }
            builder = match request.body {
                HttpBody::Json(value) => builder.json(&value),
                HttpBody::Multipart { fields, file } => {
                    let mut form = reqwest::multipart::Form::new();
                    for (name, value) in fields {
                        form = form.text(name, value);
                    }
                    let (field, filename, bytes) = file;
                    form = form.part(
                        field,
                        reqwest::multipart::Part::bytes(bytes).file_name(filename),
                    );
                    builder.multipart(form)
                }
            };
            let response = builder.send().await.map_err(|e| e.to_string())?;
            let status = response.status();
            let bytes = response.bytes().await.map_err(|e| e.to_string())?;
            if !status.is_success() {
                return Err(format!("HTTP {status}: {}", String::from_utf8_lossy(&bytes)));
            }
            Ok(bytes.to_vec())
        })
    }
}
