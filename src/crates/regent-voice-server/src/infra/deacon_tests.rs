//! Unit tests for `deacon` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

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
                    assert_eq!(msg["params"]["conversation_key"], VOICE_CONVERSATION_KEY);
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

#[tokio::test]
async fn beginning_a_butler_call_creates_a_distinct_unkeyed_session() {
    let (client_io, server_io) = tokio::io::duplex(4096);
    let (cr, cw) = tokio::io::split(client_io);
    let (sr, mut sw) = tokio::io::split(server_io);
    let rpc = DeaconRpc::from_io(cr, cw);
    let (seen_tx, mut seen_rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        let mut lines = BufReader::new(sr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let msg: Value = serde_json::from_str(&line).unwrap();
            let id = msg.get("id").cloned().unwrap_or(Value::Null);
            let method = msg["method"].as_str().unwrap();
            seen_tx
                .send((method.to_owned(), msg["params"].clone()))
                .unwrap();
            let response = match method {
                "session.create" => {
                    json!({"jsonrpc":"2.0","id":id,"result":{"session_id":"new-call"}})
                }
                other => panic!("unexpected method {other}"),
            };
            sw.write_all(format!("{response}\n").as_bytes())
                .await
                .unwrap();
        }
    });

    assert_eq!(rpc.begin_session().await.as_deref(), Some("new-call"));
    let (method, params) = seen_rx.recv().await.unwrap();
    assert_eq!(method, "session.create");
    assert_eq!(
        params,
        json!({}),
        "new Butler calls must not reuse the permanent key"
    );
    assert_eq!(rpc.ensure_session().await.as_deref(), Some("new-call"));
    assert!(
        tokio::time::timeout(Duration::from_millis(50), seen_rx.recv())
            .await
            .is_err(),
        "turns inside the same Butler opening reuse its session"
    );
}

#[tokio::test]
async fn sync_model_tracks_the_exact_provider_pair_and_skips_redundant_sets() {
    let (client_io, server_io) = tokio::io::duplex(4096);
    let (cr, cw) = tokio::io::split(client_io);
    let (sr, mut sw) = tokio::io::split(server_io);
    let rpc = DeaconRpc::from_io(cr, cw);
    let (seen_tx, mut seen_rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        let mut lines = BufReader::new(sr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let msg: Value = serde_json::from_str(&line).unwrap();
            let id = msg.get("id").cloned().unwrap_or(Value::Null);
            let method = msg["method"].as_str().unwrap();
            seen_tx.send(method.to_owned()).unwrap();
            let response = match method {
                "model.get" => {
                    json!({"jsonrpc":"2.0","id":id,"result":{"model":"nvidia/nvidia/current"}})
                }
                "model.set" => {
                    assert_eq!(msg["params"]["model"], "openrouter/openai/gpt-next");
                    json!({"jsonrpc":"2.0","id":id,"result":{"model":"openrouter/openai/gpt-next"}})
                }
                other => panic!("unexpected method {other}"),
            };
            sw.write_all(format!("{response}\n").as_bytes())
                .await
                .unwrap();
        }
    });

    rpc.sync_model("nvidia/nvidia/current").await.unwrap();
    assert_eq!(seen_rx.recv().await.as_deref(), Some("model.get"));
    rpc.sync_model("openrouter/openai/gpt-next").await.unwrap();
    assert_eq!(seen_rx.recv().await.as_deref(), Some("model.set"));
    rpc.sync_model("openrouter/openai/gpt-next").await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(50), seen_rx.recv())
            .await
            .is_err()
    );
}
