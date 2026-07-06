//! Newline-delimited JSON-RPC 2.0 client over the deacon's stdio — a port of
//! the voice server's `DeaconRpc`. A reader task demuxes responses (matched by
//! id) from streamed notifications; notifications are forwarded verbatim to a
//! `notify` sink (the bridge wires that to a Tauri event). `dead` flips when
//! stdout closes so the command layer can report the outage instead of hanging.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{oneshot, Mutex};

/// Default for request/response-class calls; turn-length methods pass their
/// own ceiling via `request_with_timeout` (see commands::request_timeout).
#[cfg(test)]
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub struct DeaconRpc {
    writer: Mutex<Box<dyn AsyncWrite + Send + Unpin>>,
    pending: Arc<StdMutex<HashMap<i64, oneshot::Sender<Value>>>>,
    next_id: AtomicI64,
    /// Set when the deacon's stdout closes (it exited or was killed).
    dead: Arc<AtomicBool>,
}

impl DeaconRpc {
    /// Client over arbitrary async pipes — the child's stdio in production, an
    /// in-memory duplex in tests. `notify` receives every non-response line.
    pub fn from_io(
        reader: impl AsyncRead + Send + Unpin + 'static,
        writer: impl AsyncWrite + Send + Unpin + 'static,
        notify: impl Fn(Value) + Send + 'static,
    ) -> Arc<Self> {
        let pending: Arc<StdMutex<HashMap<i64, oneshot::Sender<Value>>>> =
            Arc::new(StdMutex::new(HashMap::new()));
        let pending_reader = Arc::clone(&pending);
        let dead = Arc::new(AtomicBool::new(false));
        let dead_reader = Arc::clone(&dead);
        tokio::spawn(async move {
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(msg) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                match response_id(&msg) {
                    Some(id) => {
                        if let Some(tx) = pending_reader.lock().unwrap().remove(&id) {
                            tx.send(msg).ok();
                        }
                    }
                    // Not a response → a streamed notification. Forward it
                    // verbatim so the webview keeps `session_id` and can filter.
                    None => notify(msg),
                }
            }
            dead_reader.store(true, Ordering::SeqCst);
            // Surface the backend's death to the UI (respawn/notify hook).
            notify(json!({ "method": "deacon.exited", "params": {} }));
        });
        Arc::new(Self {
            writer: Mutex::new(Box::new(writer)),
            pending,
            next_id: AtomicI64::new(0),
            dead,
        })
    }

    /// True once the deacon's pipe has closed (it exited or was killed).
    #[must_use]
    pub fn is_dead(&self) -> bool {
        self.dead.load(Ordering::SeqCst)
    }

    fn next_id(&self) -> i64 {
        self.next_id.fetch_add(1, Ordering::SeqCst) + 1
    }

    async fn write_line(&self, req: &Value) -> Result<(), String> {
        let line = format!("{req}\n");
        let mut w = self.writer.lock().await;
        w.write_all(line.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        w.flush().await.map_err(|e| e.to_string())
    }

    /// Test convenience — production callers go through `request_with_timeout`
    /// so every method carries an explicit ceiling.
    #[cfg(test)]
    pub async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        self.request_with_timeout(method, params, REQUEST_TIMEOUT).await
    }

    /// `prompt.submit`'s response only arrives when the whole turn ends, so it
    /// needs a ceiling matched to the deacon's 600s turn stall limit, not the
    /// request/response default.
    pub async fn request_with_timeout(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        if self.is_dead() {
            return Err("deacon process is not running".into());
        }
        let id = self.next_id();
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        let req = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        if let Err(e) = self.write_line(&req).await {
            self.pending.lock().unwrap().remove(&id);
            return Err(e);
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(_)) => Err("deacon closed the connection before responding".into()),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(format!(
                    "deacon did not respond to {method} within {}s",
                    timeout.as_secs()
                ))
            }
        }
    }

    /// Signal EOF to the deacon by closing our end of its stdin — the first
    /// step of the graceful drain (it then flushes and exits).
    pub async fn close_stdin(&self) {
        self.writer.lock().await.shutdown().await.ok();
    }
}

/// A JSON-RPC response is a line carrying a numeric `id` AND a `result` or
/// `error`. Our own outbound requests never echo back, so a bare method line
/// (even one with an id) is treated as a notification, not a response — the
/// same rule as the voice server's `classify()`.
fn response_id(msg: &Value) -> Option<i64> {
    let id = msg.get("id").and_then(Value::as_i64)?;
    if msg.get("result").is_some() || msg.get("error").is_some() {
        Some(id)
    } else {
        None
    }
}

#[cfg(test)]
mod rpc_tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    /// Scripted server: for each request it emits a notification then the
    /// response. Exercises id demux + verbatim notification forwarding (incl.
    /// session_id) without a real binary.
    #[tokio::test]
    async fn demuxes_response_and_forwards_notifications() {
        let (client_io, server_io) = tokio::io::duplex(4096);
        let (cr, cw) = tokio::io::split(client_io);
        let (sr, mut sw) = tokio::io::split(server_io);

        let seen = Arc::new(StdMutex::new(Vec::<Value>::new()));
        let seen_writer = Arc::clone(&seen);
        let rpc = DeaconRpc::from_io(cr, cw, move |v| seen_writer.lock().unwrap().push(v));

        tokio::spawn(async move {
            let mut lines = BufReader::new(sr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let msg: Value = serde_json::from_str(&line).unwrap();
                let id = msg.get("id").cloned().unwrap_or(Value::Null);
                let note =
                    json!({"method":"message.delta","params":{"session_id":"s1","text":"hi"}});
                sw.write_all(format!("{note}\n").as_bytes()).await.unwrap();
                let resp = json!({"jsonrpc":"2.0","id":id,"result":{"ok":true}});
                sw.write_all(format!("{resp}\n").as_bytes()).await.unwrap();
            }
        });

        let v = rpc.request("status.get", json!({})).await.unwrap();
        assert_eq!(v["id"], json!(1));
        assert_eq!(v["result"]["ok"], json!(true));

        // The notification was written before the response, so the reader has
        // already forwarded it by now; a short yield covers scheduling slack.
        tokio::time::sleep(Duration::from_millis(50)).await;
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0]["params"]["session_id"], json!("s1"));
    }
}
