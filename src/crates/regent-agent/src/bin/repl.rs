//! Minimal smoke-test REPL for the core (the Go CLI replaces this later).
//!
//! Env: REGENT_API_KEY (required), REGENT_MODEL (required),
//!      REGENT_BASE_URL (default: https://openrouter.ai/api).

use regent_agent::{Agent, AgentConfig};
use regent_providers::{OpenAiCompatChat, OpenAiCompatChatConfig};
use regent_tools::{ApprovalDecision, ApprovalHandler, ToolContext, core_catalog};
use std::io::Write;
use std::sync::Arc;

const SYSTEM_PROMPT: &str = "You are Regent, a capable AI agent with terminal, file, \
and search tools. Use tools to take action; be concise.";

struct StdinApproval;

#[async_trait::async_trait]
impl ApprovalHandler for StdinApproval {
    async fn request(&self, tool: &str, action: &str, reason: &str) -> ApprovalDecision {
        let prompt = format!("\n⚠ {tool} wants to run a dangerous action ({reason}):\n  {action}\nApprove? [y/N] ");
        let answer = tokio::task::spawn_blocking(move || {
            print!("{prompt}");
            std::io::stdout().flush().ok();
            let mut line = String::new();
            std::io::stdin().read_line(&mut line).ok();
            line
        })
        .await
        .unwrap_or_default();
        if answer.trim().eq_ignore_ascii_case("y") {
            ApprovalDecision::Approve
        } else {
            ApprovalDecision::Deny
        }
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
    let api_key = std::env::var("REGENT_API_KEY").map_err(|_| "REGENT_API_KEY not set")?;
    let model = std::env::var("REGENT_MODEL").map_err(|_| "REGENT_MODEL not set")?;
    let base_url =
        std::env::var("REGENT_BASE_URL").unwrap_or_else(|_| "https://openrouter.ai/api".into());

    let home = regent_home()?;
    std::fs::create_dir_all(&home)?;
    let store = Arc::new(regent_store::Store::open(&home.join("state.db"))?);
    let graph = Arc::new(regent_graph::GraphMemory::new(Arc::clone(&store)));
    let skills = Arc::new(regent_skills::SkillLibrary::new(Arc::new(
        regent_skills::FsSkillRepository::new(home.join("skills"))?,
    )));
    let provider = Arc::new(OpenAiCompatChat::new(OpenAiCompatChatConfig::new(
        base_url, api_key, model,
    )));
    let context = ToolContext::new(std::env::current_dir()?, Arc::new(StdinApproval));

    let mut catalog = core_catalog();
    regent_tools::register_memory_tools(&mut catalog, Arc::clone(&graph), Arc::clone(&store))?;
    regent_tools::register_skill_tools(&mut catalog, Arc::clone(&skills))?;
    // Delegation: children get a leaf catalog (core tools only — no
    // delegate, no memory) and only their task brief.
    regent_agent::DelegateTool::new(
        provider.clone(),
        Arc::clone(&store),
        Arc::new(core_catalog()),
        regent_agent::DelegationConfig::default(),
    )
    .register(&mut catalog)?;

    // Reviewer whitelist: memory + skill tools only (the learning loop).
    let mut review_catalog = regent_tools::ToolCatalog::new();
    regent_tools::register_memory_tools(&mut review_catalog, Arc::clone(&graph), Arc::clone(&store))?;
    regent_tools::register_skill_tools(&mut review_catalog, Arc::clone(&skills))?;

    // Frozen snapshot: memory + skills index enter the prompt once, at
    // session start (stable + volatile tiers).
    let skills_index = skills.render_index()?;
    let system_prompt =
        format!("{SYSTEM_PROMPT}\n\n{skills_index}\n\n{}", graph.render_prompt_block()?);
    let mut agent = Agent::new(
        provider.clone(),
        Arc::new(catalog),
        Arc::clone(&store),
        context,
        system_prompt,
        AgentConfig::default(),
    )?
    .with_graph_memory(graph)
    .with_background_review(regent_agent::ReviewSetup {
        catalog: Arc::new(review_catalog),
        system_prompt: regent_skills::REVIEW_SYSTEM_PROMPT.to_owned(),
        max_iterations: 8,
    });

    // Cron: jobs in ~/.regent/cron/jobs.json tick every 30 s under the
    // file lock; runs get a fresh agent (cron source, no memory/review).
    let cron_repo = Arc::new(regent_cron::FsJobRepository::new(home.join("cron"))?);
    let cron_runner = Arc::new(regent_agent::AgentJobRunner::new(
        provider.clone(),
        Arc::new(core_catalog()),
        Arc::clone(&store),
        ToolContext::new(std::env::current_dir()?, Arc::new(StdinApproval)),
        "You are Regent running a scheduled job. Do the task, then summarize the result.",
    ));
    tokio::spawn(async move {
        let scheduler = regent_cron::Scheduler::new(
            cron_repo,
            cron_runner,
            regent_cron::SchedulerConfig::default(),
        );
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            match scheduler.tick(regent_store::now_epoch()).await {
                Ok(outcomes) => {
                    for outcome in outcomes {
                        eprintln!("[cron] {}: {:?} — {}", outcome.job_name, outcome.status, outcome.summary);
                    }
                }
                Err(error) => eprintln!("[cron] tick failed: {error}"),
            }
        }
    });

    println!("regent-core REPL — session {} (/quit to exit)", agent.session_id());
    let stdin = std::io::stdin();
    loop {
        print!("\n> ");
        std::io::stdout().flush()?;
        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            break;
        }
        let input = line.trim();
        if input.is_empty() {
            continue;
        }
        if input == "/quit" || input == "/exit" {
            break;
        }
        // Skill slash commands: "/name task". The skill body rides the
        // user message (never the cached system prompt) — Hermes pattern.
        let turn_text = match input.strip_prefix('/') {
            Some(rest) => {
                let (name, task) = rest.split_once(' ').unwrap_or((rest, ""));
                match skills.view(name) {
                    Ok(record) => {
                        let _ = skills.record_use(name);
                        format!(
                            "[Skill loaded: {name}]\n{}\n\n[User request]\n{}",
                            record.body,
                            if task.is_empty() { "Follow the skill." } else { task }
                        )
                    }
                    Err(_) => {
                        let known = skills.list().unwrap_or_default();
                        eprintln!(
                            "unknown skill '/{name}'. Available: {}",
                            known.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ")
                        );
                        continue;
                    }
                }
            }
            None => input.to_owned(),
        };
        match agent.run_turn(&turn_text).await {
            Ok(reply) => println!("\n{reply}"),
            Err(error) => eprintln!("\n[turn failed] {error}"),
        }
    }
    // Let an in-flight background review finish before exit.
    if let Some(handle) = agent.take_review_handle() {
        let _ = handle.await;
    }
    Ok(())
}

fn regent_home() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    if let Ok(custom) = std::env::var("REGENT_HOME") {
        return Ok(custom.into());
    }
    let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME"))?;
    Ok(std::path::PathBuf::from(home).join(".regent"))
}
