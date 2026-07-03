//! The agentic brain: a newline-delimited JSON-RPC 2.0 client over the
//! regent-deacon's stdio — the SAME transport the CLI uses. Port of
//! web_call.py's `_DaemonRpc`: a reader task demuxes responses (by id) from
//! streamed notifications; `stream_turn` yields the reply token-by-token and
//! a new turn first interrupts any in-flight one (latest-wins).

use crate::domain::rpc::{RpcEvent, classify};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, mpsc, oneshot};

pub struct DeaconRpc {
    writer: Mutex<Box<dyn AsyncWrite + Send + Unpin>>,
    pending: Arc<StdMutex<HashMap<i64, oneshot::Sender<Value>>>>,
    events: Mutex<mpsc::UnboundedReceiver<RpcEvent>>,
    next_id: AtomicI64,
    session: Mutex<Option<String>>,
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
            etx.send(RpcEvent::End(Some("deacon exited".into()))).ok();
        });
        Arc::new(Self {
            writer: Mutex::new(Box::new(writer)),
            pending,
            events: Mutex::new(erx),
            next_id: AtomicI64::new(0),
            session: Mutex::new(None),
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

    pub async fn ensure_session(&self) -> Option<String> {
        let mut session = self.session.lock().await;
        if session.is_none() {
            let resp = self
                .call("session.create", json!({}), Duration::from_secs(30))
                .await?;
            *session = resp
                .get("result")
                .and_then(|r| r.get("session_id"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
        }
        session.clone()
    }

    /// Stream one turn's reply deltas into `deltas`; the channel closes when
    /// the turn ends. Latest-wins: any in-flight turn is interrupted first so
    /// an abandoned turn can never block the next.
    pub async fn stream_turn(&self, text: &str, deltas: mpsc::UnboundedSender<String>) {
        let Some(sid) = self.ensure_session().await else {
            return;
        };
        let resp = self
            .call(
                "turn.interrupt",
                json!({"session_id": sid}),
                Duration::from_secs(5),
            )
            .await;
        let cancelled = resp
            .as_ref()
            .and_then(|r| r.get("result"))
            .and_then(|r| r.get("cancelled"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut events = self.events.lock().await;
        drain(&mut events, cancelled).await;
        let id = self.next_id();
        if self
            .write(
                "prompt.submit",
                &json!({"session_id": sid, "text": text}),
                Some(id),
            )
            .await
            .is_err()
        {
            return;
        }
        let mut spoke = false;
        let mut full = String::new();
        loop {
            let Ok(event) = tokio::time::timeout(Duration::from_secs(180), events.recv()).await
            else {
                return; // deacon stalled — stop rather than hang the call
            };
            match event {
                Some(RpcEvent::Delta(d)) if !d.is_empty() => {
                    spoke = true;
                    deltas.send(d).ok();
                }
                Some(RpcEvent::Reply(r)) => full = r,
                Some(RpcEvent::End(err)) => {
                    if !spoke && !full.is_empty() {
                        deltas.send(full).ok(); // provider didn't stream → once
                    } else if !spoke {
                        // The turn produced no text. If it failed (e.g. an
                        // out-of-credits 402, already humanized by the deacon),
                        // SPEAK the reason — otherwise the caller just hears
                        // silence and the call reads as stuck on "listening".
                        if let Some(reason) = err.filter(|e| !e.trim().is_empty()) {
                            deltas.send(reason).ok();
                        }
                    }
                    return;
                }
                None => {
                    if !spoke && !full.is_empty() {
                        deltas.send(full).ok();
                    }
                    return;
                }
                _ => {}
            }
        }
    }
}

/// Clear stale items from a superseded turn: consume up to its `End` when it
/// was actually cancelled, otherwise just empty what's queued.
async fn drain(events: &mut mpsc::UnboundedReceiver<RpcEvent>, block_for_end: bool) {
    if block_for_end {
        let _ = tokio::time::timeout(Duration::from_secs(2), async {
            while let Some(e) = events.recv().await {
                if matches!(e, RpcEvent::End(_)) {
                    break;
                }
            }
        })
        .await;
    } else {
        while events.try_recv().is_ok() {}
    }
}

/// Locate the regent-deacon binary: `REGENT_DEACON_PATH`, then
/// `target/{release,debug}` walking up from the current dir/exe, then PATH.
#[must_use]
pub fn find_deacon() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("REGENT_DEACON_PATH") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let name = if cfg!(windows) {
        "regent-deacon.exe"
    } else {
        "regent-deacon"
    };
    let mut bases: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        bases.extend(cwd.ancestors().map(PathBuf::from));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        bases.extend(dir.ancestors().map(PathBuf::from));
    }
    for base in &bases {
        for profile in ["release", "debug"] {
            let cand = base.join("target").join(profile).join(name);
            if cand.exists() {
                return Some(cand);
            }
        }
    }
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|d| d.join(name))
        .find(|c| c.exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scripted deacon: reads request lines from the client and answers with
    /// canned session/turn traffic — exercises demux + latest-wins end-to-end
    /// without a real binary.
    #[tokio::test]
    async fn stream_turn_yields_deltas_then_closes() {
        let (client_io, server_io) = tokio::io::duplex(4096);
        let (cr, cw) = tokio::io::split(client_io);
        let (sr, mut sw) = tokio::io::split(server_io);
        let rpc = DeaconRpc::from_io(cr, cw);

        tokio::spawn(async move {
            let mut lines = BufReader::new(sr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let msg: Value = serde_json::from_str(&line).unwrap();
                let id = msg.get("id").cloned().unwrap_or(Value::Null);
                match msg["method"].as_str().unwrap() {
                    "session.create" => {
                        let r = json!({"jsonrpc":"2.0","id":id,"result":{"session_id":"s1"}});
                        sw.write_all(format!("{r}\n").as_bytes()).await.unwrap();
                    }
                    "turn.interrupt" => {
                        let r = json!({"jsonrpc":"2.0","id":id,"result":{"cancelled":false}});
                        sw.write_all(format!("{r}\n").as_bytes()).await.unwrap();
                    }
                    "prompt.submit" => {
                        for line in [
                            json!({"method":"message.delta","params":{"text":"Hel"}}),
                            json!({"method":"message.delta","params":{"text":"lo."}}),
                            json!({"method":"message.complete","params":{"reply":"Hello."}}),
                            json!({"method":"turn.complete","params":{}}),
                        ] {
                            sw.write_all(format!("{line}\n").as_bytes()).await.unwrap();
                        }
                    }
                    other => panic!("unexpected method {other}"),
                }
            }
        });

        let (dtx, mut drx) = mpsc::unbounded_channel();
        rpc.stream_turn("hi", dtx).await;
        let mut got = Vec::new();
        while let Some(d) = drx.recv().await {
            got.push(d);
        }
        assert_eq!(got, ["Hel", "lo."]);
    }
}
