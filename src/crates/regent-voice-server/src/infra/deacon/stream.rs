//! Streaming a chat turn over the deacon RPC (delta fan-out + notification
//! routing). Split from `deacon.rs` (file-size rule).

use super::*;

impl DeaconRpc {
    /// Stream one turn's reply deltas into `deltas`; the channel closes when
    /// the turn ends. Latest-wins: any in-flight turn is interrupted first so
    /// an abandoned turn can never block the next.
    pub async fn stream_turn(&self, text: &str, deltas: mpsc::UnboundedSender<String>) {
        self.stream_turn_with_questions(text, deltas, None).await;
    }

    /// As [`stream_turn`], plus a channel for structured questions the agent
    /// asks mid-turn. `None` drops them, which is what a caller that cannot
    /// render one should do — the deacon then times the question out and the
    /// model proceeds on its own judgment.
    ///
    /// [`stream_turn`]: DeaconRpc::stream_turn
    pub async fn stream_turn_with_questions(
        &self,
        text: &str,
        deltas: mpsc::UnboundedSender<String>,
        questions: Option<mpsc::UnboundedSender<Value>>,
    ) {
        let questions = questions.unwrap_or_else(|| mpsc::unbounded_channel().0);
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
        // Only OUR session's stream feeds the call: a background job or cron
        // turn in the same deacon streams deltas too, and speaking those would
        // garble the reply. Empty sid (older deacon, exit sentinel) passes.
        let ours = |s: &str| s.is_empty() || s == sid;
        loop {
            // 600s matches turn.rs's STALL_TIMEOUT — a shorter limit here would
            // still kill long tool runs even though the turn loop allows them.
            let Ok(event) = tokio::time::timeout(Duration::from_secs(600), events.recv()).await
            else {
                return; // deacon stalled — stop rather than hang the call
            };
            match event {
                Some(RpcEvent::Delta(s, d)) if ours(&s) && !d.is_empty() => {
                    spoke = true;
                    deltas.send(d).ok();
                }
                Some(RpcEvent::Reply(s, r)) if ours(&s) => full = r,
                // A question pauses the turn until the caller answers. Hand it
                // straight to the emitter, which puts a card on screen and
                // speaks the prompt — the mic stays open throughout, so
                // answering out loud is always available.
                Some(RpcEvent::Question(s, q)) if ours(&s) => {
                    questions.send(q).ok();
                }
                Some(RpcEvent::End(s, err)) if ours(&s) => {
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
                if matches!(e, RpcEvent::End(..)) {
                    break;
                }
            }
        })
        .await;
    } else {
        while events.try_recv().is_ok() {}
    }
}
