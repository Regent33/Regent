//! Feishu file/image upload (WebhookFileSender). Split from `feishu.rs`
//! (file-size rule).

use super::*;

/// Feishu file send is two calls: upload the bytes to `im/v1/files` (returns a
/// `file_key`), then send a `file` message referencing it. Needs the tenant
/// access token. A non-empty caption follows as a separate text message.
#[async_trait]
impl WebhookFileSender for FeishuAdapter {
    async fn send_file(
        &self,
        client: &reqwest::Client,
        chat_id: &str,
        path: &Path,
        caption: &str,
    ) -> Result<(), GatewayError> {
        let token = self.tenant_token.as_deref().ok_or_else(|| {
            GatewayError::Transport("feishu tenant token not configured".to_owned())
        })?;
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|e| GatewayError::Transport(format!("read {}: {e}", path.display())))?;
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_owned();

        // 1. Upload as a generic file (file_type "stream") → file_key.
        let part = reqwest::multipart::Part::bytes(bytes).file_name(filename.clone());
        let form = reqwest::multipart::Form::new()
            .text("file_type", "stream")
            .text("file_name", filename)
            .part("file", part);
        let resp = client
            .post("https://open.feishu.cn/open-apis/im/v1/files")
            .bearer_auth(token)
            .multipart(form)
            .send()
            .await
            .map_err(|e| GatewayError::Transport(e.to_string()))?;
        let parsed: Value = resp
            .json()
            .await
            .map_err(|e| GatewayError::Parse(e.to_string()))?;
        let file_key = parsed
            .get("data")
            .and_then(|d| d.get("file_key"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                GatewayError::Transport(format!("feishu file upload failed: {parsed}"))
            })?;

        // 2. Send a file message (content is a JSON string per the API).
        let body = json!({
            "receive_id": chat_id,
            "msg_type": "file",
            "content": json!({ "file_key": file_key }).to_string(),
        });
        client
            .post(SEND_URL)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|e| GatewayError::Transport(e.to_string()))?;
        if !caption.is_empty() {
            let _ = client
                .post(SEND_URL)
                .bearer_auth(token)
                .json(&json!({
                    "receive_id": chat_id,
                    "msg_type": "text",
                    "content": json!({ "text": caption }).to_string(),
                }))
                .send()
                .await;
        }
        Ok(())
    }
}
