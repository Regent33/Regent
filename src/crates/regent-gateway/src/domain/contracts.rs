use crate::domain::entities::{MessageEvent, OutboundMessage};
use crate::domain::errors::GatewayError;
use async_trait::async_trait;
use regent_kernel::RegentError;
use std::path::Path;
use tokio_util::sync::CancellationToken;

/// One messaging platform (Telegram, webhook, …). Pull model: the runner
/// loops on `next_event`. Adapters normalize platform payloads into
/// [`MessageEvent`] and never see routing/auth logic.
#[async_trait]
pub trait PlatformAdapter: Send + Sync {
    fn platform(&self) -> &str;

    /// Blocks until the next inbound message arrives.
    async fn next_event(&self) -> Result<MessageEvent, GatewayError>;

    async fn send(&self, message: OutboundMessage) -> Result<(), GatewayError>;

    /// Upload a local file to `chat_id` with an optional caption. Defaults to
    /// "unsupported" so only platforms that implement an upload path advertise
    /// it; text delivery (`send`) is unaffected.
    async fn send_file(
        &self,
        _chat_id: &str,
        _path: &Path,
        _caption: &str,
    ) -> Result<(), GatewayError> {
        Err(GatewayError::Transport(format!(
            "file attachments are not supported on {}",
            self.platform()
        )))
    }

    /// Put an emoji reaction on a message in `chat_id`. `message_id` is
    /// `None` for "the message that started this turn" — the adapter resolves
    /// it from the last inbound message it saw in that chat, because the
    /// platform's own message id never leaves the adapter.
    ///
    /// Defaults to declining, exactly like [`PlatformAdapter::send_file`], so
    /// the platforms with no reaction API keep compiling and keep working —
    /// and the model is told in words rather than silently doing nothing.
    async fn react(
        &self,
        _chat_id: &str,
        _message_id: Option<&str>,
        _emoji: &str,
    ) -> Result<(), GatewayError> {
        Err(GatewayError::Transport(format!(
            "{} has no message reactions",
            self.platform()
        )))
    }

    /// Show a transient "typing"/working indicator in `chat_id` (the chat-native
    /// "thinking" cue while a turn runs). Best-effort; defaults to a no-op so
    /// platforms without one are unaffected. The runner refreshes it on a timer.
    async fn send_typing(&self, _chat_id: &str) -> Result<(), GatewayError> {
        Ok(())
    }
}

/// A push (webhook) messaging platform — Messenger, LINE, WhatsApp, … — where
/// the platform POSTs events to our HTTP listener rather than us polling. The
/// parse/verify/build steps are **pure** (unit-testable without a token); only
/// the network send needs live credentials. `verify` guards against spoofed
/// webhooks (constant-time signature check) and runs before `parse_webhook`.
pub trait WebhookAdapter: Send + Sync {
    fn platform(&self) -> &str;

    /// Verifies the platform's signature over the raw request body. Returns
    /// false on a missing/invalid signature (deny-by-default). `timestamp` is
    /// the platform's signing timestamp header (e.g. Slack's
    /// `X-Slack-Request-Timestamp`), needed by schemes that sign
    /// `timestamp:body` with a replay window; adapters that sign only the body
    /// ignore it.
    fn verify(&self, body: &[u8], signature: Option<&str>, timestamp: Option<&str>) -> bool;

    /// Verifies using the full request context. The default delegates to
    /// [`WebhookAdapter::verify`] (body + signature header), which covers every
    /// body-signing scheme. Adapters whose signature also covers the request
    /// URL or form params (Twilio signs `url + sorted(params)`) override this.
    fn verify_request(&self, request: &WebhookRequest<'_>) -> bool {
        self.verify(request.body, request.signature, request.timestamp)
    }

    /// Parses a verified webhook body into normalized events.
    fn parse_webhook(&self, body: &[u8]) -> Result<Vec<MessageEvent>, GatewayError>;

    /// Builds the outbound HTTP request that delivers a reply.
    fn send_request(&self, message: &OutboundMessage) -> SendRequest;

    /// The platform's own `(chat_id, message_id)` for each message in a
    /// verified webhook body — what a later "react to that" has to name.
    ///
    /// It is a separate pass rather than a field on [`MessageEvent`] because
    /// that field would touch every construction site of the event across the
    /// whole gateway to serve the handful of adapters with a reaction API.
    /// Defaults to none, so the platforms without one are unaffected.
    // ponytail: last-inbound-only. Ceiling: reacting to an ARBITRARY older
    // message needs the id plumbed through the event. Add it when replies and
    // edits want the same plumbing — all three do.
    fn inbound_message_ids(&self, _body: &[u8]) -> Vec<(String, String)> {
        Vec::new()
    }

    /// Request header carrying the signature `verify` checks (lower-case, e.g.
    /// `x-hub-signature-256`). `None` when the proof rides in the body
    /// (Mattermost) — the route then passes `None` as the signature.
    fn signature_header(&self) -> Option<&str>;

    /// Request header carrying the signing timestamp, for schemes that bind it
    /// (Slack's `x-slack-request-timestamp`). `None` for the rest.
    fn timestamp_header(&self) -> Option<&str> {
        None
    }

    /// Request header carrying the signing nonce (Feishu's
    /// `x-lark-request-nonce`). `None` for the rest.
    fn nonce_header(&self) -> Option<&str> {
        None
    }

    /// One-time endpoint-verification handshake on a **POST**, run **after**
    /// signature verification and **before** parsing. Platforms that prove
    /// ownership of the webhook URL by echoing a challenge in the POST body
    /// (Feishu/Slack `url_verification`) return the response body here;
    /// everything else returns `None` and proceeds to `parse_webhook`.
    fn handshake(&self, body: &[u8]) -> Option<SyncReply> {
        let _ = body;
        None
    }

    /// `GET`-based endpoint verification (WeChat/WeCom `echostr`): the platform
    /// signs the query string and expects the (possibly decrypted) challenge
    /// echoed back as `text/plain`. Returns `Some(body)` to echo on success,
    /// `None` to reject (`401`). `query` is the raw URL query string. The
    /// default rejects — most platforms don't verify over `GET`.
    fn verify_get(&self, query: &str) -> Option<String> {
        let _ = query;
        None
    }

    /// Most webhook platforms ack `200` and the reply is delivered out-of-band
    /// via [`WebhookAdapter::send_request`]. A few (Teams Outgoing Webhook,
    /// Google Chat, Twilio Voice) instead expect the reply **in the HTTP
    /// response body** — for those, return `true` so the route runs the turn
    /// inline and returns [`WebhookAdapter::sync_response`] rather than spawning
    /// a delivery.
    fn sync_reply(&self) -> bool {
        false
    }

    /// The HTTP response body carrying the reply for a [`sync_reply`] platform.
    /// Unused by the default (async, `send_request`) delivery path.
    ///
    /// [`sync_reply`]: WebhookAdapter::sync_reply
    fn sync_response(&self, reply: &str) -> SyncReply {
        SyncReply::Json(serde_json::json!({ "text": reply }))
    }

    /// Response when a [`sync_reply`] adapter parsed **no** user event (e.g.
    /// Twilio Voice's initial call, before any speech is captured). `None`
    /// (default) just acks `200`; Voice returns its greeting + a `<Gather>`.
    ///
    /// [`sync_reply`]: WebhookAdapter::sync_reply
    fn sync_idle_response(&self) -> Option<SyncReply> {
        None
    }
}

/// File/media upload for a [`WebhookAdapter`]. Kept separate from the (pure,
/// sync) `WebhookAdapter` trait because uploads are inherently async,
/// multi-step (most platforms: upload media → send a message referencing it),
/// and need a live HTTP client. Only platforms with an upload/media API
/// implement it; the shared webhook executor injects its `reqwest::Client` so
/// adapters stay stateless (secrets only) and the request-building stays
/// unit-testable.
#[async_trait]
pub trait WebhookFileSender: Send + Sync {
    /// Uploads the local file at `path` to `chat_id` with an optional
    /// `caption`. `client` is the executor's shared HTTP client.
    async fn send_file(
        &self,
        client: &reqwest::Client,
        chat_id: &str,
        path: &Path,
        caption: &str,
    ) -> Result<(), GatewayError>;
}

/// Emoji reactions for a [`WebhookAdapter`], kept separate from the pure, sync
/// adapter trait for exactly the reasons [`WebhookFileSender`] is: the call is
/// async and needs a live client. It is also separate from `WebhookFileSender`
/// because the two are independently supported — Slack does both, LINE does
/// neither, and a platform should not have to fake one to offer the other.
///
/// `message_id` is `None` for "the message that started this turn"; the
/// implementation resolves it from the last inbound message seen in that chat.
#[async_trait]
pub trait WebhookReactor: Send + Sync {
    /// Puts `emoji` on a message in `chat_id`. `client` is the executor's
    /// shared HTTP client.
    async fn react(
        &self,
        client: &reqwest::Client,
        chat_id: &str,
        message_id: Option<&str>,
        emoji: &str,
    ) -> Result<(), GatewayError>;
}

/// The raw HTTP request context handed to [`WebhookAdapter::verify_request`].
/// `url` is the full request URL the platform called (scheme://host/path?query),
/// needed by schemes (Twilio) that fold it into the signature; body-only
/// schemes ignore it.
pub struct WebhookRequest<'a> {
    pub url: &'a str,
    pub body: &'a [u8],
    pub signature: Option<&'a str>,
    pub timestamp: Option<&'a str>,
    /// The platform's signing nonce (Feishu's `X-Lark-Request-Nonce`, WeCom's
    /// `nonce` query param). `None` for schemes that don't use one.
    pub nonce: Option<&'a str>,
}

/// How a reply request authenticates. Most platforms use a bearer token;
/// Twilio and Azure DevOps use HTTP Basic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendAuth {
    None,
    Bearer(String),
    Basic { username: String, password: String },
}

/// A reply request's body and its wire encoding. JSON for most platforms;
/// form-urlencoded for Twilio's REST API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendBody {
    Json(serde_json::Value),
    Form(Vec<(String, String)>),
}

/// A platform-agnostic description of the HTTP call that delivers a reply —
/// built purely (testable), executed by a thin shared sender.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendRequest {
    pub url: String,
    pub auth: SendAuth,
    pub body: SendBody,
}

/// The synchronous HTTP reply body for a [`WebhookAdapter::sync_reply`] platform,
/// with its wire encoding: JSON for Teams/Google Chat, XML (TwiML) for Twilio
/// Voice. The route renders it with the matching `Content-Type`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncReply {
    Json(serde_json::Value),
    Xml(String),
}

/// The agent side of the gateway, kept behind a contract so the runner is
/// testable without models. One implementation maps session keys to
/// per-conversation agents.
#[async_trait]
pub trait ConversationHandler: Send + Sync {
    /// Runs one turn for the session; `cancel` is fired by `/stop`.
    async fn handle(
        &self,
        session_key: &str,
        text: &str,
        cancel: CancellationToken,
    ) -> Result<String, RegentError>;

    /// `/new` — drop the session so the next message starts fresh.
    async fn reset(&self, session_key: &str);
}
