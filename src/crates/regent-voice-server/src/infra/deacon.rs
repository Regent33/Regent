//! The agentic brain: a newline-delimited JSON-RPC 2.0 client over the
//! regent-deacon's stdio — the SAME transport the CLI uses. Port of
//! web_call.py's `_DaemonRpc`: a reader task demuxes responses (by id) from
//! streamed notifications; `stream_turn` yields the reply token-by-token and
//! a new turn first interrupts any in-flight one (latest-wins).

use crate::domain::rpc::{RpcEvent, classify};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, mpsc, oneshot};

mod stream;

const VOICE_CONVERSATION_KEY: &str = "butler:voice";
/// Butler can draw a question card, so it says so. Without this the deacon
/// deliberately withholds `question.request` and flattens every structured
/// question to numbered text a caller would have to answer by reciting a
/// number — see ADR-046.
const QUESTION_CAPABILITY: [&str; 1] = ["questions"];

pub struct DeaconRpc {
    writer: Mutex<Box<dyn AsyncWrite + Send + Unpin>>,
    pending: Arc<StdMutex<HashMap<i64, oneshot::Sender<Value>>>>,
    events: Mutex<mpsc::UnboundedReceiver<RpcEvent>>,
    next_id: AtomicI64,
    session: Mutex<Option<String>>,
    /// Last model confirmed on this child. Butler rereads main chat's persisted
    /// provider/model before every turn, but only sends `model.set` on change.
    model: Mutex<Option<String>>,
    /// Set when the deacon's stdout closes (process died) — the turn path
    /// checks this and respawns instead of echoing forever.
    dead: Arc<std::sync::atomic::AtomicBool>,
}

impl DeaconRpc {
    /// Client over arbitrary async pipes — the child's stdio in production,
    /// an in-memory duplex in tests.
    pub fn from_io(
        reader: impl AsyncRead + Send + Unpin + 'static,
        writer: impl AsyncWrite + Send + Unpin + 'static,
    ) -> Arc<Self> {
        let (etx, erx) = mpsc::unbounded_channel();
        let pending: Arc<StdMutex<HashMap<i64, oneshot::Sender<Value>>>> =
            Arc::new(StdMutex::new(HashMap::new()));
        let pending_reader = Arc::clone(&pending);
        let dead = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let dead_reader = Arc::clone(&dead);
        tokio::spawn(async move {
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(msg) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                match classify(&msg) {
                    RpcEvent::Response(id) => {
                        if let Some(tx) = pending_reader.lock().unwrap().remove(&id) {
                            tx.send(msg).ok();
                        }
                    }
                    RpcEvent::Ignore => {}
                    event => {
                        etx.send(event).ok();
                    }
                }
            }
            dead_reader.store(true, Ordering::SeqCst);
            etx.send(RpcEvent::End(String::new(), Some("deacon exited".into())))
                .ok();
        });
        Arc::new(Self {
            writer: Mutex::new(Box::new(writer)),
            pending,
            events: Mutex::new(erx),
            next_id: AtomicI64::new(0),
            session: Mutex::new(None),
            model: Mutex::new(None),
            dead,
        })
    }

    /// True once the deacon's pipe has closed (it exited or was killed).
    #[must_use]
    pub fn is_dead(&self) -> bool {
        self.dead.load(Ordering::SeqCst)
    }

    async fn write(&self, method: &str, params: &Value, id: Option<i64>) -> Result<(), ()> {
        let mut req = json!({"jsonrpc": "2.0", "method": method, "params": params});
        if let Some(id) = id {
            req["id"] = json!(id);
        }
        let line = format!("{req}\n");
        let mut w = self.writer.lock().await;
        w.write_all(line.as_bytes()).await.map_err(|_| ())?;
        w.flush().await.map_err(|_| ())
    }

    fn next_id(&self) -> i64 {
        self.next_id.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Send a request and await its response (`None` on timeout/write error).
    pub async fn call(&self, method: &str, params: Value, timeout: Duration) -> Option<Value> {
        let id = self.next_id();
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        if self.write(method, &params, Some(id)).await.is_err() {
            self.pending.lock().unwrap().remove(&id);
            return None;
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(v)) => Some(v),
            _ => {
                self.pending.lock().unwrap().remove(&id);
                None
            }
        }
    }

    /// Hand a typed questionnaire answer back to the paused turn. Called by
    /// `/call/answer` when the caller taps a card in Butler.
    pub async fn answer_question(&self, answer: Value) -> bool {
        let Some(sid) = self.session.lock().await.clone() else {
            return false;
        };
        self.call(
            "question.respond",
            json!({"session_id": sid, "answer": answer}),
            Duration::from_secs(10),
        )
        .await
        .and_then(|r| r.get("result")?.get("resolved")?.as_bool())
        .unwrap_or(false)
    }

    pub async fn ensure_session(&self) -> Option<String> {
        let mut session = self.session.lock().await;
        if session.is_none() {
            let resp = self
                .call(
                    "session.create",
                    json!({
                        "conversation_key": VOICE_CONVERSATION_KEY,
                        "capabilities": QUESTION_CAPABILITY,
                    }),
                    Duration::from_secs(30),
                )
                .await?;
            *session = resp
                .get("result")
                .and_then(|r| r.get("session_id"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
        }
        session.clone()
    }

    /// Start one distinct Butler call session. The voice server outlives the
    /// modal, so a permanent `butler:voice` binding collapses every opening
    /// into one stale rail row/title. Interrupt the prior call, then create an
    /// unkeyed session; subsequent turns reuse it through `ensure_session`.
    pub async fn begin_session(&self) -> Option<String> {
        let mut session = self.session.lock().await;
        if let Some(previous) = session.as_deref() {
            let _ = self
                .call(
                    "turn.interrupt",
                    json!({"session_id": previous}),
                    Duration::from_secs(5),
                )
                .await;
        }
        let response = self
            .call(
                "session.create",
                json!({"capabilities": QUESTION_CAPABILITY}),
                Duration::from_secs(30),
            )
            .await?;
        *session = response
            .get("result")
            .and_then(|result| result.get("session_id"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        session.clone()
    }

    /// Match this child deacon to main chat's exact provider/model pair. The
    /// first call probes its current model; later calls are no-ops until the
    /// persisted chat selection changes.
    pub async fn sync_model(&self, desired: &str) -> Result<(), String> {
        let desired = desired.trim();
        if desired.is_empty() || desired.len() > 512 || desired.chars().any(char::is_control) {
            return Err("invalid Butler model selection".into());
        }
        let mut selected = self.model.lock().await;
        if selected.as_deref() == Some(desired) {
            return Ok(());
        }
        if selected.is_none() {
            let response = self
                .call("model.get", json!({}), Duration::from_secs(5))
                .await
                .ok_or_else(|| "the Butler agent did not report its model".to_owned())?;
            if let Some(error) = response.get("error") {
                return Err(format!("could not read the Butler model: {error}"));
            }
            let current = response
                .get("result")
                .and_then(|result| result.get("model"))
                .and_then(Value::as_str);
            if current == Some(desired) {
                *selected = Some(desired.to_owned());
                return Ok(());
            }
        }
        let response = self
            .call(
                "model.set",
                json!({"model": desired}),
                Duration::from_secs(10),
            )
            .await
            .ok_or_else(|| "the Butler agent did not accept the selected model".to_owned())?;
        if let Some(error) = response.get("error") {
            return Err(format!("could not apply the chat model to Butler: {error}"));
        }
        let applied = response
            .get("result")
            .and_then(|result| result.get("model"))
            .and_then(Value::as_str);
        if applied != Some(desired) {
            return Err("the Butler agent acknowledged a different model".into());
        }
        *selected = Some(desired.to_owned());
        Ok(())
    }
}

#[cfg(test)]
#[path = "deacon_tests.rs"]
mod tests;
