//! Provider/speech construction for the gateway bin.
//! Split from gateway.rs (file-size rule).

use super::*;

/// One resolved provider in the gateway's failover chain (as `regent gateway
/// start` resolves it from config.yaml + .env, primary-first).
#[derive(serde::Deserialize)]
struct ChainLink {
    base_url: String,
    api_key: String,
    model: String,
}

/// The gateway's chat provider. Prefers `REGENT_PROVIDER_CHAIN` — a JSON array
/// of `{base_url, api_key, model}` the CLI resolves from the config `providers`
/// map + `agents_defaults` (primary then fallbacks, links with a missing key
/// dropped). That mirrors the deacon's routing, so the gateway fails over the
/// same way and rotating keys / a changed model take effect on the next start.
/// Falls back to the single `REGENT_API_KEY`/`REGENT_BASE_URL`/`REGENT_MODEL`
/// provider when no chain is present (a plain single-provider setup).
pub(crate) fn build_provider() -> Result<Arc<dyn ChatProvider>, Box<dyn std::error::Error>> {
    if let Ok(raw) = std::env::var("REGENT_PROVIDER_CHAIN")
        && !raw.trim().is_empty()
    {
        match serde_json::from_str::<Vec<ChainLink>>(&raw) {
            Ok(links) => {
                let chain: Vec<Arc<dyn ChatProvider>> = links
                    .into_iter()
                    .filter(|l| !l.api_key.trim().is_empty() && !l.model.trim().is_empty())
                    .map(|l| {
                        Arc::new(OpenAiCompatChat::new(OpenAiCompatChatConfig::new(
                            l.base_url, l.api_key, l.model,
                        ))) as Arc<dyn ChatProvider>
                    })
                    .collect();
                match chain.len() {
                    0 => tracing::warn!("REGENT_PROVIDER_CHAIN had no usable links; using single"),
                    1 => return Ok(chain.into_iter().next().unwrap()),
                    _ => return Ok(Arc::new(FallbackChat::new(chain)?)),
                }
            }
            Err(e) => tracing::warn!(%e, "REGENT_PROVIDER_CHAIN is not valid JSON; using single"),
        }
    }
    let api_key = std::env::var("REGENT_API_KEY").map_err(|_| "REGENT_API_KEY not set")?;
    let model = std::env::var("REGENT_MODEL").map_err(|_| "REGENT_MODEL not set")?;
    let base_url =
        std::env::var("REGENT_BASE_URL").unwrap_or_else(|_| "https://openrouter.ai/api".into());
    Ok(Arc::new(OpenAiCompatChat::new(
        OpenAiCompatChatConfig::new(base_url, api_key, model),
    )))
}

/// Build the voice ASR/TTS pair from env, or `None` when voice isn't configured
/// (no `REGENT_SPEECH_BASE_URL`). One OpenAI-compatible adapter serves both —
/// point `base_url` at a hosted endpoint (Groq/OpenAI/DashScope) or a localhost
/// Qwen3 server. The reqwest executor captures the runtime handle, so this must
/// run inside the async `run()`.
pub(crate) fn build_speech() -> Option<(Arc<dyn AsrProvider>, Arc<dyn TtsProvider>)> {
    let base = std::env::var("REGENT_SPEECH_BASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())?;
    let key = std::env::var("REGENT_SPEECH_API_KEY").unwrap_or_default();
    let provider = std::env::var("REGENT_SPEECH_PROVIDER").unwrap_or_else(|_| "speech".to_owned());
    let asr_model =
        std::env::var("REGENT_SPEECH_ASR_MODEL").unwrap_or_else(|_| "qwen3-asr-1.7b".to_owned());
    let tts_model =
        std::env::var("REGENT_SPEECH_TTS_MODEL").unwrap_or_else(|_| "qwen3-tts-1.7b".to_owned());
    let exec = Arc::new(ReqwestExecutor::new());
    let asr: Arc<dyn AsrProvider> = Arc::new(OpenAiCompatAsr::new(
        provider.clone(),
        base.clone(),
        key.clone(),
        asr_model,
        Arc::clone(&exec),
    ));
    let tts: Arc<dyn TtsProvider> =
        Arc::new(OpenAiCompatTts::new(provider, base, key, tts_model, exec));
    Some((asr, tts))
}

pub(crate) fn regent_home() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Ok(custom) = std::env::var("REGENT_HOME") {
        return Ok(custom.into());
    }
    let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME"))?;
    Ok(PathBuf::from(home).join(".regent"))
}
