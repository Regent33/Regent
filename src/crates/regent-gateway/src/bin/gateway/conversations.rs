//! The gateway's conversation handler + outbound delivery sink.
//! Split from gateway.rs (file-size rule).

use super::*;

pub(crate) struct AgentConversations {
    pub(crate) provider: Arc<dyn ChatProvider>,
    pub(crate) store: Arc<regent_store::Store>,
    pub(crate) graph: Arc<regent_graph::GraphMemory>,
    pub(crate) skills: Arc<regent_skills::SkillLibrary>,
    pub(crate) adapter: Arc<dyn PlatformAdapter>,
    pub(crate) approvals: Arc<ApprovalRouter>,
    pub(crate) cwd: PathBuf,
    pub(crate) jobs: Arc<dyn regent_cron::JobRepository>,
    pub(crate) sessions: tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<Agent>>>>,
}

/// Bridges the agent's `send_message`/`send_file` tools to the platform adapter,
/// bound to one chat. Text goes via `send`; files via `send_file` (per-adapter).
pub(crate) struct PlatformDelivery {
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

        // Tool parity with the deacon: a chat session is a full Regent
        // session, not a cut-down one. `core_catalog_from_env` (not
        // `core_catalog`) so REGENT_TERMINAL_BACKEND/REGENT_SANDBOX and the
        // REGENT_COMPUTER_USE flag are honoured here exactly as they are for
        // the desktop app — the gateway silently lacking computer_use is why
        // the model fell back to hand-written automation scripts.
        let mut catalog = core_catalog_from_env()?;
        register_memory_tools(
            &mut catalog,
            Arc::clone(&self.graph),
            Arc::clone(&self.store),
        )?;
        register_skill_tools(&mut catalog, Arc::clone(&self.skills))?;
        register_persona_tool(&mut catalog, Arc::clone(&self.store))?;
        register_key_tool(&mut catalog)?;
        register_kanban_tool(
            &mut catalog,
            Arc::clone(&self.store),
            "regent".to_owned(),
            "regent".to_owned(),
        )?;
        // Scheduled work, bound to this chat so its results come back here
        // rather than into a log the user will never read.
        register_schedule_tool(
            &mut catalog,
            Arc::clone(&self.jobs),
            Some(format!("{platform}:{chat_id}")),
        )?;
        // Browser control via Playwright MCP (opt-in: REGENT_BROWSER_MCP_URL);
        // best-effort, mutating actions approval-gated.
        regent_tools::attach_browser_if_configured(&mut catalog).await;
        // send_message/send_file → through the platform adapter to *this* chat.
        let delivery = Arc::new(PlatformDelivery {
            adapter: Arc::clone(&self.adapter),
            chat_id: chat_id.clone(),
        });
        register_message_tool(&mut catalog, Arc::clone(&delivery) as Arc<dyn DeliverySink>)?;
        register_file_tool(&mut catalog, delivery as Arc<dyn DeliverySink>)?;
        regent_agent::DelegateTool::new(
            Arc::clone(&self.provider),
            Arc::clone(&self.store),
            Arc::new(core_catalog_from_env()?),
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

        // REGENT_NOW is stamped once, when the gateway process starts — a
        // gateway that has been up for a week would otherwise assert last
        // week's date as the present moment. Say when it started and point at
        // the tool for the actual time (the deacon's prompt does the same).
        let now = format!(
            "\n\nThis gateway started {} (the user's local time). Real time has passed since \
             then — for today's date, the current time, or anything time-sensitive, call the \
             current_time tool instead of assuming.",
            chrono::Local::now().format("%A, %B %e, %Y at %I:%M %p")
        );
        // Per-object artifacts area under the real REGENT_HOME (env else
        // ~/.regent), mirroring the deacon — never cwd-relative, so a missing
        // env can't make the agent invent a `.regent/` folder in the repo.
        let artifacts = {
            let dir = regent_home()
                .map(|h| h.join("artifacts"))
                .unwrap_or_else(|_| PathBuf::from(".regent").join("artifacts"));
            format!(
                "\n\nWhen you generate a new standalone artifact or file (screenshots included — \
                 not edits to the user's existing files), create a dedicated folder for it under \
                 {} — one subfolder per object, e.g. {}{}<short-slug>{} — and put its files \
                 there. Never create files elsewhere for these.\
                 \n\nTHEN SEND IT: you are talking to the user over chat, on another device. A \
                 local path is useless to them — they cannot open it. Whenever you produce a \
                 file they asked for, or they ask you to send/show one, call `send_file` with \
                 its path. Only mention a path in addition to sending, never instead of it.",
                dir.display(),
                dir.display(),
                std::path::MAIN_SEPARATOR,
                std::path::MAIN_SEPARATOR,
            )
        };
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
            min_new_messages: 8,
            // Gateway currently receives one resolved chat provider; inherit
            // it for reviews until the composition root exposes model.review.
            provider: None,
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
