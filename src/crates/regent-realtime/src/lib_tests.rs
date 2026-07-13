//! Unit tests for `lib` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

struct EchoTools;
#[async_trait]
impl CallTools for EchoTools {
    async fn invoke(&self, name: &str, args: &Value) -> String {
        format!("ran {name}({args})")
    }
}

fn frame(n: i16) -> AudioFrame {
    AudioFrame {
        pcm: vec![n],
        sample_rate: 24_000,
    }
}

#[tokio::test]
async fn relays_audio_both_ways_and_runs_tool_calls() {
    let (caller_tx, caller_rx) = mpsc::channel(8); // caller → engine
    let (out_tx, mut out_rx) = mpsc::channel(8); // engine → caller
    let (pin_tx, mut pin_rx) = mpsc::channel(8); // engine → provider
    let (ev_tx, ev_rx) = mpsc::channel(8); // provider → engine
    let (tr_tx, mut tr_rx) = mpsc::channel(8); // engine → provider (tool results)

    let engine = tokio::spawn(run_call(
        TransportEnds {
            audio_in: caller_rx,
            audio_out: out_tx,
        },
        ProviderEnds {
            audio_in: pin_tx,
            events: ev_rx,
            tool_results: tr_tx,
        },
        Arc::new(EchoTools),
    ));

    // caller audio reaches the provider
    caller_tx.send(frame(1)).await.unwrap();
    assert_eq!(pin_rx.recv().await.unwrap(), frame(1));

    // provider audio reaches the caller
    ev_tx.send(ProviderEvent::Audio(frame(2))).await.unwrap();
    assert_eq!(out_rx.recv().await.unwrap(), frame(2));

    // a provider tool-call is executed and its result returned to the provider
    ev_tx
        .send(ProviderEvent::ToolCall {
            id: "t1".into(),
            name: "weather".into(),
            args: serde_json::json!({ "city": "Pampanga" }),
        })
        .await
        .unwrap();
    let result = tr_rx.recv().await.unwrap();
    assert_eq!(result.id, "t1");
    assert!(result.output.contains("ran weather"));

    // closing both sides ends the call cleanly
    drop(ev_tx);
    drop(caller_tx);
    engine.await.unwrap().unwrap();
}

#[tokio::test]
async fn transport_close_ends_the_call() {
    let (caller_tx, caller_rx) = mpsc::channel::<AudioFrame>(1);
    let (out_tx, _out_rx) = mpsc::channel(1);
    let (pin_tx, _pin_rx) = mpsc::channel(1);
    let (_ev_tx, ev_rx) = mpsc::channel(1);
    let (tr_tx, _tr_rx) = mpsc::channel(1);
    let engine = tokio::spawn(run_call(
        TransportEnds {
            audio_in: caller_rx,
            audio_out: out_tx,
        },
        ProviderEnds {
            audio_in: pin_tx,
            events: ev_rx,
            tool_results: tr_tx,
        },
        Arc::new(EchoTools),
    ));
    drop(caller_tx); // caller hung up
    engine.await.unwrap().unwrap();
}
