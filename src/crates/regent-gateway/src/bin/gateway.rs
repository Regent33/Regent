//! Runnable Telegram gateway (composition root — canonical `app/di`).
//!
//! Env: REGENT_TELEGRAM_TOKEN, REGENT_API_KEY, REGENT_MODEL (required);
//!      REGENT_BASE_URL (default OpenRouter);
//!      REGENT_TELEGRAM_ALLOWED_USERS (comma-separated numeric ids) or
//!      REGENT_TELEGRAM_ALLOW_ALL=1.
//! Voice (opt-in — set REGENT_SPEECH_BASE_URL to enable): voice notes are
//!      transcribed and replies spoken back. REGENT_SPEECH_API_KEY (empty for a
//!      localhost server), REGENT_SPEECH_ASR_MODEL / REGENT_SPEECH_TTS_MODEL
//!      (default qwen3-asr-1.7b / qwen3-tts-1.7b), REGENT_SPEECH_PROVIDER (label).
//! Pairing state persists to ~/.regent/gateway-auth.json.

use async_trait::async_trait;
use regent_agent::{Agent, AgentConfig, CAPABILITIES, ReviewSetup, SYSTEM_PROMPT};
use regent_gateway::{
    ApprovalRouter, AuthPolicy, ChatApprovalHandler, ConversationHandler, GatewayRunner,
    OutboundMessage, PlatformAdapter, RateLimiter, ReqwestExecutor, TelegramAdapter,
};
use regent_kernel::{AsrProvider, RegentError, TtsProvider};
use regent_providers::{ChatProvider, FallbackChat, OpenAiCompatChat, OpenAiCompatChatConfig};
use regent_speech::{OpenAiCompatAsr, OpenAiCompatTts};
use regent_tools::{
    DeliverySink, ToolCatalog, ToolContext, core_catalog, register_file_tool, register_key_tool,
    register_memory_tools, register_persona_tool, register_skill_tools,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

struct AgentConversations {
    provider: Arc<dyn ChatProvider>,
    store: Arc<regent_store::Store>,
    graph: Arc<regent_graph::GraphMemory>,
    skills: Arc<regent_skills::SkillLibrary>,
    adapter: Arc<dyn PlatformAdapter>,
    approvals: Arc<ApprovalRouter>,
    cwd: PathBuf,
    sessions: tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<Agent>>>>,
}

/// Bridges the agent's `send_message`/`send_file` tools to the platform adapter,
/// bound to one chat. Text goes via `send`; files via `send_file` (per-adapter).
struct PlatformDelivery {
    adapter: Arc<dyn PlatformAdapter>,
    chat_id: String,
}

#[async_trait]
impl DeliverySink for PlatformDelivery {
    async fn deliver(&self, _target: &str, text: &str) -> Result<(), RegentError> {
        self.adapter
            .send(OutboundMessage {
                chat_id: self.chat_id.clone(),
                text: text.to_owned(),
            })
            .await
            .map_err(|e| RegentError::Tool {
                tool: "send_message".into(),
                message: e.to_string(),
            })
    }
    fn targets(&self) -> Vec<String> {
        vec![format!("{}:{}", self.adapter.platform(), self.chat_id)]
    }
    async fn deliver_file(
        &self,
        _target: &str,
        path: &std::path::Path,
        caption: &str,
    ) -> Result<(), RegentError> {
        self.adapter
            .send_file(&self.chat_id, path, caption)
            .await
            .map_err(|e| RegentError::Tool {
                tool: "send_file".into(),
                message: e.to_string(),
            })
    }
}

impl AgentConversations {
    async fn build_agent(&self, session_key: &str) -> Result<Agent, RegentError> {
        // session key format: agent:main:{platform}:{chat_id}
        let chat_id = session_key
            .rsplit(':')
            .next()
            .unwrap_or("unknown")
            .to_owned();
        let platform = self.adapter.platform().to_owned();
        let approval = Arc::new(ChatApprovalHandler::new(
            Arc::clone(&self.adapter),
            Arc::clone(&self.approvals),
            format!("{platform}:{chat_id}"),
            chat_id.clone(),
            Duration::from_secs(120),
        ));
        let context = ToolContext::new(self.cwd.clone(), approval);

        let mut catalog = core_catalog();
        register_memory_tools(
            &mut catalog,
            Arc::clone(&self.graph),
            Arc::clone(&self.store),
        )?;
        register_skill_tools(&mut catalog, Arc::clone(&self.skills))?;
        register_persona_tool(&mut catalog, Arc::clone(&self.store))?;
        register_key_tool(&mut catalog)?;
        // Browser control via Playwright MCP (opt-in: REGENT_BROWSER_MCP_URL);
        // best-effort, mutating actions approval-gated.
        regent_tools::attach_browser_if_configured(&mut catalog).await;
        // send_file → upload through the platform adapter to *this* chat.
        register_file_tool(
            &mut catalog,
            Arc::new(PlatformDelivery {
                adapter: Arc::clone(&self.adapter),
                chat_id: chat_id.clone(),
            }),
        )?;
        regent_agent::DelegateTool::new(
            Arc::clone(&self.provider),
            Arc::clone(&self.store),
            Arc::new(core_catalog()),
            regent_agent::DelegationConfig::default(),
        )
        .register(&mut catalog)?;
        let mut review_catalog = ToolCatalog::new();
        register_memory_tools(
            &mut review_catalog,
            Arc::clone(&self.graph),
            Arc::clone(&self.store),
        )?;
        register_skill_tools(&mut review_catalog, Arc::clone(&self.skills))?;
        register_persona_tool(&mut review_catalog, Arc::clone(&self.store))?;

        let now = std::env::var("REGENT_NOW")
            .ok()
            .filter(|n| !n.is_empty())
            .map(|n| format!("\n\nThe current date and time is {n} (the user's local time)."))
            .unwrap_or_default();
        // Per-object artifacts area under `.regent` (REGENT_HOME), mirroring the deacon.
        let artifacts = std::env::var("REGENT_HOME")
            .ok()
            .filter(|h| !h.is_empty())
            .map(|h| {
                let dir = std::path::Path::new(&h).join("artifacts");
                format!(
                    "\n\nWhen you generate a new standalone artifact or project (not edits to the \
                     user's existing files), create a dedicated folder for it under {} (one \
                     subfolder per object), put its files there, and tell the user the path.",
                    dir.display(),
                )
            })
            .unwrap_or_default();
        let system_prompt = format!(
            "{SYSTEM_PROMPT} You're reached over chat — keep replies concise and chat-friendly \
             (plain text, not markdown).{now}{artifacts}{}\n\n{CAPABILITIES}\n\n{}\n\n{}",
            self.store.persona_block(),
            self.skills.render_index().map_err(RegentError::from)?,
            self.graph
                .render_prompt_block()
                .map_err(RegentError::from)?,
        );
        let config = AgentConfig {
            source: "telegram".to_owned(),
            ..AgentConfig::default()
        };
        Ok(Agent::new(
            Arc::clone(&self.provider),
            Arc::new(catalog),
            Arc::clone(&self.store),
            context,
            system_prompt,
            config,
        )?
        .with_graph_memory(Arc::clone(&self.graph))
        .with_background_review(ReviewSetup {
            catalog: Arc::new(review_catalog),
            system_prompt: regent_skills::REVIEW_SYSTEM_PROMPT.to_owned(),
            max_iterations: 8,
        }))
    }
}

#[async_trait]
impl ConversationHandler for AgentConversations {
    async fn handle(
        &self,
        session_key: &str,
        text: &str,
        cancel: CancellationToken,
    ) -> Result<String, RegentError> {
        let agent_arc = {
            let mut sessions = self.sessions.lock().await;
            match sessions.get(session_key) {
                Some(existing) => Arc::clone(existing),
                None => {
                    let fresh = Arc::new(tokio::sync::Mutex::new(
                        self.build_agent(session_key).await?,
                    ));
                    sessions.insert(session_key.to_owned(), Arc::clone(&fresh));
                    fresh
                }
            }
        };
        let mut agent = agent_arc.lock().await;
        agent.reset_interrupt();
        let agent_cancel = agent.cancel_handle();
        let watcher = tokio::spawn(async move {
            cancel.cancelled().await;
            agent_cancel.cancel();
        });
        let result = agent.run_turn(text).await;
        watcher.abort();
        result
    }

    async fn reset(&self, session_key: &str) {
        self.sessions.lock().await.remove(session_key);
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    if let Err(error) = run().await {
        eprintln!("fatal: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let token =
        std::env::var("REGENT_TELEGRAM_TOKEN").map_err(|_| "REGENT_TELEGRAM_TOKEN not set")?;
    let home = regent_home()?;
    std::fs::create_dir_all(&home)?;
    std::fs::create_dir_all(home.join("artifacts"))?;

    let store = Arc::new(regent_store::Store::open(&home.join("state.db"))?);
    let graph = Arc::new(regent_graph::GraphMemory::new(Arc::clone(&store)));
    let skills = Arc::new(regent_skills::SkillLibrary::new(Arc::new(
        regent_skills::FsSkillRepository::new(home.join("skills"))?,
    )));
    let provider = build_provider()?;
    let mut telegram = TelegramAdapter::new(token);
    if let Some((asr, tts)) = build_speech() {
        println!("voice enabled (REGENT_SPEECH_BASE_URL set)");
        telegram = telegram.with_speech(asr, tts);
    }
    let adapter: Arc<dyn PlatformAdapter> = Arc::new(telegram);
    let approvals = Arc::new(ApprovalRouter::new());
    let auth = Arc::new(AuthPolicy::new(regent_gateway::load_auth_snapshot(&home)));

    // Persist pairing state once a minute (cheap, restart-safe) via the shared
    // atomic writer (tmp + rename).
    let auth_for_save = Arc::clone(&auth);
    let home_for_save = home.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let _ =
                regent_gateway::persist_auth_snapshot(&home_for_save, &auth_for_save.snapshot());
        }
    });

    let handler = Arc::new(AgentConversations {
        provider,
        store,
        graph,
        skills,
        adapter: Arc::clone(&adapter),
        approvals: Arc::clone(&approvals),
        cwd: std::env::current_dir()?,
        sessions: tokio::sync::Mutex::new(HashMap::new()),
    });

    println!("regent-gateway (telegram) up — waiting for messages");
    let rate = Arc::new(RateLimiter::from_env());
    let runner = GatewayRunner::new(adapter, handler, auth, rate, approvals);
    runner.run().await?;
    Ok(())
}

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
fn build_provider() -> Result<Arc<dyn ChatProvider>, Box<dyn std::error::Error>> {
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
    Ok(Arc::new(OpenAiCompatChat::new(OpenAiCompatChatConfig::new(
        base_url, api_key, model,
    ))))
}

/// Build the voice ASR/TTS pair from env, or `None` when voice isn't configured
/// (no `REGENT_SPEECH_BASE_URL`). One OpenAI-compatible adapter serves both —
/// point `base_url` at a hosted endpoint (Groq/OpenAI/DashScope) or a localhost
/// Qwen3 server. The reqwest executor captures the runtime handle, so this must
/// run inside the async `run()`.
fn build_speech() -> Option<(Arc<dyn AsrProvider>, Arc<dyn TtsProvider>)> {
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

fn regent_home() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Ok(custom) = std::env::var("REGENT_HOME") {
        return Ok(custom.into());
    }
    let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME"))?;
    Ok(PathBuf::from(home).join(".regent"))
}
