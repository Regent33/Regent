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
