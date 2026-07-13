//! Outbound delivery for webhook chats: the shared reqwest send, the
//! per-platform delivery adapter, and the session sink. Split from
//! `webhook.rs` (file-size rule).

use super::*;

pub(super) async fn deliver(client: &reqwest::Client, req: &SendRequest) {
    let mut builder = match &req.body {
        SendBody::Json(value) => client.post(&req.url).json(value),
        SendBody::Form(pairs) => client.post(&req.url).form(pairs),
    };
    builder = match &req.auth {
        SendAuth::None => builder,
        SendAuth::Bearer(token) => builder.bearer_auth(token),
        SendAuth::Basic { username, password } => builder.basic_auth(username, Some(password)),
    };
    if let Err(error) = builder.send().await {
        tracing::warn!(%error, url = req.url, "webhook reply delivery failed");
    }
}

/// Routes a keyed platform session's `send_message`/`send_file` back to the
/// platform's API. Built from env (adapters are stateless, so reconstructing
/// them here rather than sharing the router's registry is cheap and keeps the
/// router signature untouched).
pub struct WebhookPlatformDelivery {
    pub(in crate::infra::webhook) adapters: Registry,
    pub(in crate::infra::webhook) file_senders: HashMap<String, Arc<dyn WebhookFileSender>>,
    pub(in crate::infra::webhook) client: reqwest::Client,
}

impl WebhookPlatformDelivery {
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            // Outbound only — no inbound verification here, so use the registry
            // variant that doesn't spawn a duplicate Google Chat JWKS refresher.
            adapters: delivery_registry_from_env(),
            file_senders: file_senders_from_env(),
            client: reqwest::Client::new(),
        }
    }
}

impl PlatformDelivery for WebhookPlatformDelivery {
    fn sink_for(&self, conversation_key: &str) -> Option<Arc<dyn DeliverySink>> {
        let (platform, chat_id) = conversation_key.split_once(':')?;
        let adapter = self.adapters.get(platform)?;
        Some(Arc::new(WebhookDelivery {
            platform: platform.to_owned(),
            chat_id: chat_id.to_owned(),
            adapter: Arc::clone(adapter),
            file_sender: self.file_senders.get(platform).cloned(),
            client: self.client.clone(),
        }))
    }
}

/// One platform conversation's outbound sink: text via the adapter's
/// `send_request`, files via its [`WebhookFileSender`] (when it has one).
pub(super) struct WebhookDelivery {
    platform: String,
    chat_id: String,
    adapter: Arc<dyn WebhookAdapter>,
    file_sender: Option<Arc<dyn WebhookFileSender>>,
    client: reqwest::Client,
}

#[async_trait]
impl DeliverySink for WebhookDelivery {
    async fn deliver(&self, _target: &str, text: &str) -> Result<(), RegentError> {
        let message = OutboundMessage {
            chat_id: self.chat_id.clone(),
            text: text.to_owned(),
        };
        deliver(&self.client, &self.adapter.send_request(&message)).await;
        Ok(())
    }

    fn targets(&self) -> Vec<String> {
        vec![format!("{}:{}", self.platform, self.chat_id)]
    }

    async fn deliver_file(
        &self,
        _target: &str,
        path: &std::path::Path,
        caption: &str,
    ) -> Result<(), RegentError> {
        match &self.file_sender {
            Some(sender) => sender
                .send_file(&self.client, &self.chat_id, path, caption)
                .await
                .map_err(|e| RegentError::Tool {
                    tool: "send_file".into(),
                    message: e.to_string(),
                }),
            None => Err(RegentError::Tool {
                tool: "send_file".into(),
                message: format!("file upload is not supported on {}", self.platform),
            }),
        }
    }
}
