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
        // Per-object artifacts area under the real REGENT_HOME (env else
        // ~/.regent), mirroring the deacon — never cwd-relative, so a missing
        // env can't make the agent invent a `.regent/` folder in the repo.
        let artifacts = {
            let dir = regent_home()
                .map(|h| h.join("artifacts"))
                .unwrap_or_else(|_| PathBuf::from(".regent").join("artifacts"));
            format!(
                "\n\nWhen you generate a new standalone artifact or file to send (screenshots \
                 included — not edits to the user's existing files), create a dedicated folder \
                 for it under {} (one subfolder per object), put its files there, and tell the \
                 user the path. Never create files elsewhere for these.",
                dir.display(),
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
