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

mod conversations;
mod providers;

use conversations::AgentConversations;
use providers::{build_provider, build_speech, regent_home};

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
