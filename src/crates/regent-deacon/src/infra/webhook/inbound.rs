//! The inbound POST handler: pairing, gating, session routing, and the
//! sync-reply rendering. Split from `webhook.rs` (file-size rule).

use super::*;

pub(super) async fn handle(
    State(state): State<WebhookState>,
    Path(platform): Path<String>,
    uri: axum::http::Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(adapter) = state.registry.get(&platform).cloned() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let header = |name: Option<&str>| -> Option<String> {
        name.and_then(|n| headers.get(n))
            .and_then(|v| v.to_str().ok())
            .map(ToOwned::to_owned)
    };
    let signature = header(adapter.signature_header());
    let timestamp = header(adapter.timestamp_header());
    let nonce = header(adapter.nonce_header());

    // Reconstruct the full public URL (HTTP/1.1 request targets are origin-form,
    // so scheme/host live in proxy headers). Only URL-signing schemes (Twilio)
    // read it; body-only adapters ignore it via the default `verify_request`.
    let scheme = header(Some("x-forwarded-proto")).unwrap_or_else(|| "https".to_owned());
    let host = header(Some("x-forwarded-host"))
        .or_else(|| header(Some("host")))
        .unwrap_or_default();
    let path_and_query = uri
        .path_and_query()
        .map_or_else(|| uri.path(), |pq| pq.as_str());
    let url = format!("{scheme}://{host}{path_and_query}");

    let request = WebhookRequest {
        url: &url,
        body: body.as_ref(),
        signature: signature.as_deref(),
        timestamp: timestamp.as_deref(),
        nonce: nonce.as_deref(),
    };
    if !adapter.verify_request(&request) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    // Endpoint-verification handshake (Feishu/Slack url_verification, WeChat
    // echostr): authenticated above, answered here before any turn runs.
    if let Some(reply) = adapter.handshake(&body) {
        return render_sync(reply);
    }
    let events = match adapter.parse_webhook(&body) {
        Ok(events) => events,
        Err(error) => {
            tracing::warn!(%error, platform, "webhook parse failed");
            return StatusCode::BAD_REQUEST.into_response();
        }
    };

    // Sync-response platforms (Teams, Twilio Voice, …) expect the reply in the
    // HTTP response body: run the single turn inline and return it.
    if adapter.sync_reply() {
        let Some(event) = events.into_iter().next() else {
            // No user utterance yet (e.g. Voice's initial call): the adapter may
            // still owe a response (greeting + prompt), else just ack.
            return match adapter.sync_idle_response() {
                Some(reply) => render_sync(reply),
                None => StatusCode::OK.into_response(),
            };
        };
        // Rate-limit before authz so an unauthorized flooder is throttled too
        // (else every spam message would still cost an outbound "not authorized"
        // reply). Each user_key has its own bucket, so this only throttles the
        // sender's own flood.
        if !state.rate.check(&format!("{platform}:{}", event.user_id)) {
            return render_sync(adapter.sync_response(RATE_LIMITED_MSG));
        }
        if let Some(reply) = gate(&state, &platform, &event.user_id, &event.text) {
            return render_sync(adapter.sync_response(reply));
        }
        let key = format!("{platform}:{}", event.chat_id);
        return match state.service.chat_keyed(&key, event.text).await {
            Ok(reply) => render_sync(adapter.sync_response(&reply.reply)),
            Err(error) => {
                tracing::warn!(%error, platform, "webhook turn failed");
                StatusCode::OK.into_response()
            }
        };
    }

    // Otherwise ack fast; run turns + deliver replies off the request path.
    tokio::spawn(async move {
        for event in events {
            // Rate-limit before authz — an unauthorized flooder is throttled too.
            if !state.rate.check(&format!("{platform}:{}", event.user_id)) {
                let out = OutboundMessage {
                    chat_id: event.chat_id,
                    text: RATE_LIMITED_MSG.to_owned(),
                };
                deliver(&state.client, &adapter.send_request(&out)).await;
                continue;
            }
            if let Some(reply) = gate(&state, &platform, &event.user_id, &event.text) {
                let out = OutboundMessage {
                    chat_id: event.chat_id,
                    text: reply.to_owned(),
                };
                deliver(&state.client, &adapter.send_request(&out)).await;
                continue;
            }
            // One continuous session per platform conversation.
            let key = format!("{platform}:{}", event.chat_id);
            let reply = match state.service.chat_keyed(&key, event.text).await {
                Ok(reply) => reply.reply,
                Err(error) => {
                    tracing::warn!(%error, platform, "webhook turn failed");
                    continue;
                }
            };
            let out = OutboundMessage {
                chat_id: event.chat_id,
                text: reply,
            };
            deliver(&state.client, &adapter.send_request(&out)).await;
        }
    });
    StatusCode::OK.into_response()
}

/// Renders a sync-reply body with the matching `Content-Type` (JSON for
/// Teams/Google Chat, `text/xml` TwiML for Twilio Voice).
pub(super) fn render_sync(reply: SyncReply) -> Response {
    match reply {
        SyncReply::Json(value) => (StatusCode::OK, Json(value)).into_response(),
        SyncReply::Xml(body) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/xml; charset=utf-8")],
            body,
        )
            .into_response(),
    }
}
