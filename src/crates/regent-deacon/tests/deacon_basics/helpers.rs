//! Shared test doubles: the scripted provider and the session-manager factory.

use async_trait::async_trait;
use or_core::TokenUsage;
use regent_agent::AgentConfig;
use regent_deacon::SessionManager;
use regent_kernel::ChatMessage;
use regent_providers::{ChatProvider, ChatRequest, ChatResponse, ProviderError};
use regent_skills::{FsSkillRepository, SkillLibrary};
use regent_store::Store;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;
use tokio::sync::mpsc::unbounded_channel;

pub struct ScriptedProvider {
    pub responses: Mutex<VecDeque<ChatResponse>>,
}

impl ScriptedProvider {
    pub fn with(responses: Vec<ChatResponse>) -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(responses.into()),
        })
    }

    pub fn text_reply(text: &str) -> ChatResponse {
        ChatResponse {
            message: ChatMessage::assistant(Some(text.into()), vec![]),
            usage: TokenUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            },
            finish_reason: Some("stop".into()),
        }
    }
}

#[async_trait]
impl ChatProvider for ScriptedProvider {
    async fn complete(&self, _req: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| ProviderError::Parse("script exhausted".into()))
    }

    fn model(&self) -> &str {
        "scripted"
    }
}

pub fn make_session_manager(
    dir: &TempDir,
    provider: Arc<dyn ChatProvider>,
) -> (
    Arc<SessionManager>,
    tokio::sync::mpsc::UnboundedReceiver<String>,
) {
    let store = Arc::new(Store::open(&dir.path().join("state.db")).unwrap());
    let graph = Arc::new(regent_graph::GraphMemory::new(Arc::clone(&store)));
    let skills = Arc::new(SkillLibrary::new(Arc::new(
        FsSkillRepository::new(dir.path().join("skills")).unwrap(),
    )));
    let (tx, rx) = unbounded_channel();
    let model = provider.model().to_owned();
    let factory: regent_deacon::ProviderFactory = Arc::new(move |_model| Arc::clone(&provider));
    let sm = Arc::new(SessionManager::new(
        factory,
        model,
        store,
        graph,
        skills,
        PathBuf::from("."),
        AgentConfig::default(),
        regent_deacon::ToolsConfig::default(),
        tx,
    ));
    (sm, rx)
}
