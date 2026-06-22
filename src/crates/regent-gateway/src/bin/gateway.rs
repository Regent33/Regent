//! Runnable Telegram gateway (composition root — canonical `app/di`).
//!
//! Env: REGENT_TELEGRAM_TOKEN, REGENT_API_KEY, REGENT_MODEL (required);
//!      REGENT_BASE_URL (default OpenRouter);
//!      REGENT_TELEGRAM_ALLOWED_USERS (comma-separated numeric ids) or
//!      REGENT_TELEGRAM_ALLOW_ALL=1.
//! Pairing state persists to ~/.regent/gateway-auth.json.

use async_trait::async_trait;
use regent_agent::{Agent, AgentConfig, BASE_PROMPT, CAPABILITIES, ReviewSetup};
use regent_gateway::{
    ApprovalRouter, AuthPolicy, AuthSnapshot, ChatApprovalHandler, ConversationHandler,
    GatewayRunner, OutboundMessage, PlatformAdapter, TelegramAdapter,
};
use regent_kernel::RegentError;
use regent_providers::{ChatProvider, OpenAiCompatChat, OpenAiCompatChatConfig};
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
        // Per-object artifacts area under `.regent` (REGENT_HOME), mirroring the daemon.
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
            "{BASE_PROMPT} You're reached over chat — keep replies concise and chat-friendly \
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
    let api_key = std::env::var("REGENT_API_KEY").map_err(|_| "REGENT_API_KEY not set")?;
    let model = std::env::var("REGENT_MODEL").map_err(|_| "REGENT_MODEL not set")?;
    let base_url =
        std::env::var("REGENT_BASE_URL").unwrap_or_else(|_| "https://openrouter.ai/api".into());
    let home = regent_home()?;
    std::fs::create_dir_all(&home)?;
    std::fs::create_dir_all(home.join("artifacts"))?;

    let store = Arc::new(regent_store::Store::open(&home.join("state.db"))?);
    let graph = Arc::new(regent_graph::GraphMemory::new(Arc::clone(&store)));
    let skills = Arc::new(regent_skills::SkillLibrary::new(Arc::new(
        regent_skills::FsSkillRepository::new(home.join("skills"))?,
    )));
    let provider: Arc<dyn ChatProvider> = Arc::new(OpenAiCompatChat::new(
        OpenAiCompatChatConfig::new(base_url, api_key, model),
    ));
    let adapter: Arc<dyn PlatformAdapter> = Arc::new(TelegramAdapter::new(token));
    let approvals = Arc::new(ApprovalRouter::new());
    let auth = Arc::new(AuthPolicy::new(load_auth_snapshot(&home)));

    // Persist pairing state once a minute (cheap, restart-safe).
    let auth_for_save = Arc::clone(&auth);
    let auth_path = home.join("gateway-auth.json");
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            if let Ok(raw) = serde_json::to_string_pretty(&auth_for_save.snapshot()) {
                let _ = std::fs::write(&auth_path, raw);
            }
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
    let runner = GatewayRunner::new(adapter, handler, auth, approvals);
    runner.run().await?;
    Ok(())
}

fn load_auth_snapshot(home: &std::path::Path) -> AuthSnapshot {
    let mut snapshot: AuthSnapshot = std::fs::read_to_string(home.join("gateway-auth.json"))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    snapshot.allow_all = std::env::var("REGENT_TELEGRAM_ALLOW_ALL").is_ok_and(|v| v == "1");
    // Operators come from env on every boot (config is the source of truth).
    snapshot.allowlist = std::env::var("REGENT_TELEGRAM_ALLOWED_USERS")
        .unwrap_or_default()
        .split(',')
        .filter(|id| !id.trim().is_empty())
        .map(|id| format!("telegram:{}", id.trim()))
        .collect();
    snapshot
}

fn regent_home() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Ok(custom) = std::env::var("REGENT_HOME") {
        return Ok(custom.into());
    }
    let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME"))?;
    Ok(PathBuf::from(home).join(".regent"))
}
