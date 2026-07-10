//! Live model switch / config reload reaching OPEN sessions (routing epoch).

use crate::helpers::ScriptedProvider;
use async_trait::async_trait;
use regent_agent::AgentConfig;
use regent_deacon::SessionManager;
use regent_providers::{ChatProvider, ChatRequest, ChatResponse, ProviderError};
use regent_skills::{FsSkillRepository, SkillLibrary};
use regent_store::Store;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::mpsc::unbounded_channel;

/// Provider that answers with its own model name, so the test can see which
/// model a turn actually ran on.
struct EchoModelProvider {
    name: String,
}

#[async_trait]
impl ChatProvider for EchoModelProvider {
    async fn complete(&self, _req: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        Ok(ScriptedProvider::text_reply(&self.name))
    }

    fn model(&self) -> &str {
        &self.name
    }
}

fn make_manager_with_factory(
    dir: &TempDir,
    initial_model: &str,
    factory: regent_deacon::ProviderFactory,
) -> Arc<SessionManager> {
    let store = Arc::new(Store::open(&dir.path().join("state.db")).unwrap());
    let graph = Arc::new(regent_graph::GraphMemory::new(Arc::clone(&store)));
    let skills = Arc::new(SkillLibrary::new(Arc::new(
        FsSkillRepository::new(dir.path().join("skills")).unwrap(),
    )));
    let (tx, _rx) = unbounded_channel();
    Arc::new(SessionManager::new(
        factory,
        initial_model,
        store,
        graph,
        skills,
        PathBuf::from("."),
        AgentConfig::default(),
        regent_deacon::ToolsConfig::default(),
        tx,
    ))
}

/// `set_model` (and the config/env reload path behind `bump_routing`) must
/// apply to a session that is already open — the next turn runs on the new
/// model, not the one captured at session build.
#[tokio::test]
async fn model_switch_applies_to_open_sessions_next_turn() {
    let dir = TempDir::new().unwrap();
    // The factory honors the requested model — like the real routing snapshot.
    let factory: regent_deacon::ProviderFactory = Arc::new(|model: &str| {
        Arc::new(EchoModelProvider {
            name: model.to_owned(),
        })
    });
    let sm = make_manager_with_factory(&dir, "m-one", factory);

    let sid = sm.create_session().await.unwrap();
    let first = sm.run_turn(&sid, "hi").await.unwrap();
    assert_eq!(first, "m-one", "session starts on the initial model");

    sm.set_model("m-two");
    let second = sm.run_turn(&sid, "hi again").await.unwrap();
    assert_eq!(
        second, "m-two",
        "an open session picks up the switch on its next turn"
    );
}

/// A config/key change (config.set / env.set both funnel through `bump_routing`)
/// must re-route an OPEN session's next turn even when the MODEL string is
/// unchanged — proving key/provider edits (not just model switches) reach live
/// sessions. The factory reads a shared cell that stands in for the routing
/// snapshot a config/env change would rebuild.
#[tokio::test]
async fn config_change_reroutes_open_session_without_model_change() {
    let dir = TempDir::new().unwrap();
    // Shared "routing snapshot": the model string stays "main", but a config/key
    // change flips which provider the factory resolves that same model to.
    let resolved = Arc::new(std::sync::Mutex::new("before".to_owned()));
    let factory: regent_deacon::ProviderFactory = {
        let resolved = Arc::clone(&resolved);
        Arc::new(move |_model: &str| {
            Arc::new(EchoModelProvider {
                name: resolved.lock().unwrap().clone(),
            })
        })
    };
    let sm = make_manager_with_factory(&dir, "main", factory);

    let sid = sm.create_session().await.unwrap();
    assert_eq!(sm.run_turn(&sid, "hi").await.unwrap(), "before");

    // Stand in for config.set/env.set: mutate the snapshot, then bump routing.
    *resolved.lock().unwrap() = "after".to_owned();
    sm.bump_routing();

    assert_eq!(
        sm.run_turn(&sid, "hi again").await.unwrap(),
        "after",
        "config/key change re-routes the open session's next turn (same model id)"
    );
}
