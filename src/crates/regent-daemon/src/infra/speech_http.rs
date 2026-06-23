//! reqwest-backed [`HttpExecutor`] for the speech backends. `regent-speech`
//! builds requests purely and stays network-free; this is the one place the
//! voice stack touches the wire.
//!
//! The kernel speech traits are sync, so `execute` blocks. It is only ever
//! called from a `spawn_blocking` task (the `voice.test` handler / the gateway
//! runner offload the transcribe/synthesize call), so blocking on the stored
//! runtime `Handle` here is safe — never call it from a runtime worker thread.

use regent_speech::{HttpBody, HttpExecutor, SpeechHttpRequest};
use tokio::runtime::Handle;

pub struct ReqwestExecutor {
    client: reqwest::Client,
    handle: Handle,
}

impl ReqwestExecutor {
    /// Capture the current runtime handle. Must be constructed inside the tokio
    /// runtime (the daemon composition root is async).
    // No `Default`: construction requires an active runtime (`Handle::current`),
    // so a parameterless `default()` would panic off-runtime — a footgun.
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
                // Never send the API key (bearer) over an insecure URL — a
                // plaintext or external `base_url` would exfiltrate the key.
                if !key_safe_url(&request.url) {
                    return Err(format!(
                        "refusing to send the speech API key over an insecure URL (need HTTPS or loopback): {}",
                        request.url
                    ));
                }
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
                return Err(format!(
                    "HTTP {status}: {}",
                    String::from_utf8_lossy(&bytes)
                ));
            }
            Ok(bytes.to_vec())
        })
    }
}

/// A bearer credential may only ride HTTPS (or loopback http for a local
/// server) — anything else risks leaking the key to a plaintext/external host.
fn key_safe_url(url: &str) -> bool {
    match reqwest::Url::parse(url) {
        Ok(u) => match u.scheme() {
            "https" => true,
            "http" => matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "::1")),
            _ => false,
        },
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::key_safe_url;

    #[test]
    fn api_key_only_rides_https_or_loopback() {
        assert!(key_safe_url("https://api.groq.com/openai/v1/audio/transcriptions"));
        assert!(key_safe_url("http://localhost:8000/v1/audio/speech"));
        assert!(key_safe_url("http://127.0.0.1/x"));
        assert!(!key_safe_url("http://evil.example/x")); // plaintext external → key withheld
        assert!(!key_safe_url("ftp://x/y"));
        assert!(!key_safe_url("not a url"));
    }
}
