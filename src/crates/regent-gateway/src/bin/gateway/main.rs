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
mod cron;
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
    DeliverySink, ReactionSink, ToolCatalog, ToolContext, core_catalog_from_env,
    register_file_tool, register_kanban_tool, register_key_tool, register_memory_tools,
    register_message_tool, register_persona_tool, register_reaction_tool, register_schedule_tool,
    register_skill_tools,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Cron tick interval. One-shot jobs have a 120s catch-up grace, so this has
/// to stay comfortably under that or a "remind me at 8pm" is skipped as stale.
const CRON_TICK_SECS: u64 = 60;

fn main() {
    // A gateway spawned detached (`regent gateway start`) gets its own empty
    // console window on Windows. It sits on the desktop and lands in every
    // screenshot the agent takes — the "cmd window blocking the view", which
    // also makes the shot look "not whole". Hide it before anything else.
    hide_detached_console();

    // Default core capabilities (computer_use) on BEFORE the async runtime —
    // env writes must not race worker threads (edition 2024). This is what
    // makes chat get screen control no matter how the gateway was launched;
    // relying on the CLI launcher alone let a stale/other launch path answer
    // "computer_use isn't enabled". Runs first, single-threaded.
    regent_gateway::apply_capability_defaults();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // Manual runtime (not #[tokio::main]) so the env defaults above are set
    // while still single-threaded; the macro would start worker threads first.
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("fatal: cannot start async runtime: {error}");
            std::process::exit(1);
        }
    };
    if let Err(error) = runtime.block_on(run()) {
        eprintln!("fatal: {error}");
        std::process::exit(1);
    }
}

/// Hide the gateway's own console window when it is running detached (spawned
/// by `regent gateway start`, logs going to a file — not attached to a real
/// terminal). Guarded on `!stdout.is_terminal()` so a foreground `regent-gateway`
/// run keeps its window visible for watching logs. No-op off Windows.
#[cfg(windows)]
fn hide_detached_console() {
    use std::io::IsTerminal;
    if std::io::stdout().is_terminal() {
        return; // foreground run — leave the user's terminal alone
    }
    // Minimal Win32 FFI (kernel32/user32 are always linked on Windows); avoids
    // pulling a whole windowing crate for two calls.
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetConsoleWindow() -> isize;
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
    }
    const SW_HIDE: i32 = 0;
    // SAFETY: both are plain Win32 calls; a null handle (no console) just
    // no-ops ShowWindow. No pointers are dereferenced on our side.
    unsafe {
        let console = GetConsoleWindow();
        if console != 0 {
            ShowWindow(console, SW_HIDE);
        }
    }
}

#[cfg(not(windows))]
fn hide_detached_console() {}

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

    // Scheduled jobs share the deacon's store, so `regent cron list` sees what
    // a chat scheduled and vice versa; ownership decides who runs what.
    let jobs = Arc::new(regent_cron::FsJobRepository::new(home.join("cron"))?);
    // The same ledger the deacon writes to — one record per execution, whoever
    // ran it. Without this a chat-scheduled job left no trace at all.
    let ledger = Arc::new(regent_jobs::JobLedger::new(Arc::clone(&store)));

    let handler = Arc::new(AgentConversations {
        provider,
        store,
        graph,
        skills,
        adapter: Arc::clone(&adapter),
        approvals: Arc::clone(&approvals),
        cwd: std::env::current_dir()?,
        jobs: Arc::clone(&jobs) as Arc<dyn regent_cron::JobRepository>,
        sessions: tokio::sync::Mutex::new(HashMap::new()),
    });
    cron::spawn(
        Arc::clone(&jobs) as Arc<dyn regent_cron::JobRepository>,
        Arc::clone(&handler),
        Arc::clone(&adapter),
        ledger,
        CRON_TICK_SECS,
    );

    // Surface the resolved capability state at startup: "why can't the bot see
    // my screen?" should be answerable from the log, not by guessing at env.
    println!(
        "regent-gateway (telegram) up — waiting for messages (screen control: {})",
        if regent_tools::infra::computer_use::is_enabled() {
            "on"
        } else {
            "off — set REGENT_COMPUTER_USE=1"
        }
    );
    let rate = Arc::new(RateLimiter::from_env());
    let runner = GatewayRunner::new(adapter, handler, auth, rate, approvals);
    runner.run().await?;
    Ok(())
}
