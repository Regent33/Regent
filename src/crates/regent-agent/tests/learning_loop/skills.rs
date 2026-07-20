//! Agent-created skills persist across sessions (M3 exit criterion).

use crate::{Scripted, context, text, tool_call};
use regent_agent::{Agent, AgentConfig};
use regent_skills::{FsSkillRepository, SkillLibrary};
use regent_store::Store;
use regent_tools::{ToolCatalog, register_skill_tools};
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn agent_created_skill_persists_and_loads_next_session() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("state.db")).unwrap());
    let skills_root = dir.path().join("skills");
    let library = Arc::new(SkillLibrary::new(Arc::new(
        FsSkillRepository::new(&skills_root).unwrap(),
    )));

    let mut catalog = ToolCatalog::new();
    register_skill_tools(&mut catalog, Arc::clone(&library)).unwrap();

    let provider = Scripted::new(vec![
        tool_call(
            "skill_manage",
            json!({
                "action": "create", "name": "release-checklist",
                "description": "Release checklist for the api service.",
                "body": "# Steps\n1. tag\n2. build\n3. announce"
            }),
        ),
        text("skill saved"),
    ]);
    let mut agent = Agent::new(
        provider,
        Arc::new(catalog),
        store,
        context(),
        "system",
        AgentConfig::default(),
    )
    .unwrap();
    assert_eq!(
        agent
            .run_turn("save what we learned as a skill")
            .await
            .unwrap(),
        "skill saved"
    );

    // "Next session": a fresh library over the same root sees the skill and
    // serves it through every disclosure level.
    let next_session_library =
        SkillLibrary::new(Arc::new(FsSkillRepository::new(&skills_root).unwrap()));
    let index = next_session_library.render_index().unwrap();
    assert!(index.contains("- release-checklist: Release checklist for the api service."));
    let record = next_session_library.view("release-checklist").unwrap();
    assert!(record.body.contains("announce"));
    assert_eq!(record.meta.created_by, "agent");
}
