//! Real conditional-GET behavior against a local ephemeral server (127.0.0.1:0)
//! — no live network, no `$REGENT_HOME`. Exercises ETag replay → 304, the body
//! size cap, and non-success status reporting through the actual reqwest client.

use super::fetch::{FetchOutcome, conditional_get};
use super::model::MAX_MANIFEST_BYTES;
use super::parse_manifest;
use axum::{
    Router,
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const ETAG: &str = "\"deadbeef\"";
const BODY: &str = r#"{"schema":1,"channels":{"stable":{"version":"0.1.2"}}}"#;

async fn manifest(headers: HeaderMap) -> impl IntoResponse {
    let matched = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        == Some(ETAG);
    if matched {
        return (StatusCode::NOT_MODIFIED, [(header::ETAG, ETAG)], Vec::new());
    }
    (
        StatusCode::OK,
        [(header::ETAG, ETAG)],
        BODY.as_bytes().to_vec(),
    )
}

async fn oversized() -> impl IntoResponse {
    (StatusCode::OK, vec![b'x'; MAX_MANIFEST_BYTES + 1])
}

/// Bind an ephemeral port, serve `app` in the background, return its base URL.
/// The runtime cancels the server task when the test returns.
async fn spawn_server(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}")
}

async fn spawn_chunked_oversized() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let Ok((mut socket, _)) = listener.accept().await else {
            return;
        };
        let mut request = [0_u8; 1024];
        let _ = socket.read(&mut request).await;
        let _ = socket
            .write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
            .await;
        let chunk = vec![b'x'; 8 * 1024];
        for _ in 0..(MAX_MANIFEST_BYTES / chunk.len() + 2) {
            let header = format!("{:X}\r\n", chunk.len());
            if socket.write_all(header.as_bytes()).await.is_err()
                || socket.write_all(&chunk).await.is_err()
                || socket.write_all(b"\r\n").await.is_err()
            {
                break;
            }
        }
        let _ = socket.write_all(b"0\r\n\r\n").await;
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn fetch_returns_body_and_etag_then_304_on_replay() {
    let base = spawn_server(Router::new().route("/m", get(manifest))).await;
    let url = format!("{base}/m");
    let client = reqwest::Client::new();

    match conditional_get(&client, &url, None).await {
        FetchOutcome::Fetched { body, etag } => {
            let m = parse_manifest(&body).expect("body parses");
            assert_eq!(m.stable_version().unwrap().to_string(), "0.1.2");
            assert_eq!(etag.as_deref(), Some(ETAG));
        }
        _ => panic!("expected Fetched on first GET"),
    }

    // Replaying the ETag as If-None-Match must yield a 304.
    assert!(matches!(
        conditional_get(&client, &url, Some(ETAG)).await,
        FetchOutcome::NotModified
    ));
}

#[tokio::test]
async fn oversized_response_body_is_rejected() {
    let base = spawn_server(Router::new().route("/big", get(oversized))).await;
    let url = format!("{base}/big");
    match conditional_get(&reqwest::Client::new(), &url, None).await {
        FetchOutcome::Failed(reason) => assert!(reason.contains("too large"), "{reason}"),
        _ => panic!("expected Failed(too large)"),
    }
}

#[tokio::test]
async fn chunked_oversized_body_is_stopped_while_streaming() {
    let url = spawn_chunked_oversized().await;
    match conditional_get(&reqwest::Client::new(), &url, None).await {
        FetchOutcome::Failed(reason) => assert!(reason.contains("too large"), "{reason}"),
        _ => panic!("expected Failed(too large)"),
    }
}

#[tokio::test]
async fn non_success_status_is_reported() {
    let base = spawn_server(Router::new().route("/m", get(manifest))).await;
    let url = format!("{base}/missing");
    match conditional_get(&reqwest::Client::new(), &url, None).await {
        FetchOutcome::Failed(reason) => assert!(reason.contains("404"), "{reason}"),
        _ => panic!("expected Failed(404)"),
    }
}
