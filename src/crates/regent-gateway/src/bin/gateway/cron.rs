//! Scheduled jobs for chat surfaces.
//!
//! The deacon's cron loop logs its summaries and stops there, which is fine
//! for a local job but useless for "remind me at 8pm" typed into Telegram —
//! the answer has to come back to the chat that asked. So the gateway runs
//! its own scheduler over the SAME `$REGENT_HOME/cron` store, claiming only
//! the jobs stamped with its platform (`CronJob::target`), and delivers each
//! result to the originating chat.

use super::*;
use regent_cron::{CronError, CronJob, JobRepository, JobRunner, Scheduler, SchedulerConfig};

/// Runs a due job as a turn in the chat that scheduled it, then delivers the
/// reply there. Reusing the live session (rather than a fresh cron agent) is
/// deliberate: a reminder that has the conversation in context is the whole
/// point, and it costs no extra wiring.
struct ChatJobRunner {
    conversations: Arc<AgentConversations>,
    adapter: Arc<dyn PlatformAdapter>,
    platform: String,
}

#[async_trait]
impl JobRunner for ChatJobRunner {
    async fn run(&self, job: &CronJob) -> Result<String, CronError> {
        let Some(chat_id) = job
            .target
            .as_deref()
            .and_then(|t| t.rsplit(':').next())
            .filter(|c| !c.is_empty())
        else {
            return Err(CronError::RunFailed(format!(
                "job '{}' has no chat to answer",
                job.name
            )));
        };
        let key = regent_gateway::build_session_key(&self.platform, chat_id);
        // The job prompt is the user's own instruction, delivered late; the
        // preamble keeps the model from re-asking questions nobody is there
        // to answer.
        let prompt = format!(
            "[scheduled task '{}' — firing now, the user is not necessarily present] {}",
            job.name, job.prompt
        );
        let text = self
            .conversations
            .handle(&key, &prompt, CancellationToken::new())
            .await?;
        if !text.trim().is_empty() {
            self.adapter
                .send(OutboundMessage {
                    chat_id: chat_id.to_owned(),
                    text: text.clone(),
                })
                .await
                .map_err(|e| CronError::RunFailed(e.to_string()))?;
        }
        Ok(text)
    }
}

/// Spawn the tick loop. Ticks are cheap (a file read under a lock) and the
/// shared tick lock keeps this process and the deacon off each other's toes.
///
/// The runner is wrapped in [`regent_jobs::LedgerCronRunner`] for the same
/// reason the deacon's is: every execution gets a durable row with the four
/// completion facts, and `regent jobs` can see it. Chat-scheduled runs used to
/// land only in this process's log — the deacon's ledger had no idea they had
/// happened, which made "one ledger for all work" true of only one surface.
pub(crate) fn spawn(
    jobs: Arc<dyn JobRepository>,
    conversations: Arc<AgentConversations>,
    adapter: Arc<dyn PlatformAdapter>,
    ledger: Arc<regent_jobs::JobLedger>,
    tick_secs: u64,
) {
    let platform = adapter.platform().to_owned();
    let runner = Arc::new(
        regent_jobs::LedgerCronRunner::new(
            Arc::new(ChatJobRunner {
                conversations,
                adapter,
                platform: platform.clone(),
            }),
            ledger,
        )
        .with_budget(regent_jobs::CRON_BUDGET_SECS),
    );
    tokio::spawn(async move {
        let scheduler = Scheduler::new(
            jobs,
            runner,
            SchedulerConfig {
                owner: Some(platform),
                hard_timeout_secs: regent_jobs::CRON_WATCHDOG_SECS,
                ..SchedulerConfig::default()
            },
        );
        loop {
            tokio::time::sleep(Duration::from_secs(tick_secs)).await;
            match scheduler.tick(regent_store::now_epoch()).await {
                Ok(outcomes) => {
                    for o in outcomes {
                        tracing::info!(job = o.job_name, status = ?o.status, "gateway cron tick");
                    }
                }
                Err(error) => tracing::warn!(%error, "gateway cron tick failed"),
            }
        }
    });
}
