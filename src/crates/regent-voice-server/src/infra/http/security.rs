//! Host + CORS gate for the loopback-only server.

use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, Request, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// The origins allowed to call cross-origin: the regent-web call UI (Next,
/// :3000) plus an optional `REGENT_CALL_UI_ORIGIN`. Never a wildcard.
fn origin_allowed(origin: &str) -> bool {
    if matches!(origin, "http://localhost:3000" | "http://127.0.0.1:3000") {
        return true;
    }
    std::env::var("REGENT_CALL_UI_ORIGIN").is_ok_and(|o| o.trim_end_matches('/') == origin)
}

/// Host + CORS gate. Rejects non-local Hosts — a page on `evil.tld` that
/// resolves to 127.0.0.1 (DNS rebinding) still sends `Host: evil.tld`.
/// Reflects CORS headers only for [`origin_allowed`] origins and answers
/// their preflights; every other origin gets no grant, so its scripts can't
/// read anything (the browser blocks it).
pub(super) async fn security(req: Request<Body>, next: Next) -> Response {
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default();
    let name = host.rsplit_once(':').map_or(host, |(n, _)| n);
    if !matches!(name, "localhost" | "127.0.0.1" | "[::1]") {
        return (StatusCode::FORBIDDEN, "local requests only").into_response();
    }
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|o| o.to_str().ok())
        .filter(|o| origin_allowed(o))
        .map(ToOwned::to_owned);
    if req.method() == axum::http::Method::OPTIONS {
        let mut res = StatusCode::NO_CONTENT.into_response();
        if origin.is_some() {
            cors_headers(res.headers_mut(), origin.as_deref());
            res.headers_mut().insert(
                header::ACCESS_CONTROL_ALLOW_METHODS,
                HeaderValue::from_static("GET, POST"),
            );
            res.headers_mut().insert(
                header::ACCESS_CONTROL_ALLOW_HEADERS,
                HeaderValue::from_static("content-type, x-call-token"),
            );
        }
        return res;
    }
    let mut res = next.run(req).await;
    cors_headers(res.headers_mut(), origin.as_deref());
    res
}

fn cors_headers(headers: &mut HeaderMap, origin: Option<&str>) {
    headers.insert(header::VARY, HeaderValue::from_static("Origin"));
    if let Some(o) = origin
        && let Ok(value) = o.parse()
    {
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
    }
}
