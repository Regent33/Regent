//! `update_persona` — the agent edits its own identity/behavior (soul) and what
//! it knows about the user (profile). Stored in the DB; full effect next session
//! (the system prompt is frozen per session), so the agent should acknowledge.
//! Persona is per-user (per-home), not shared, so no approval gate.

use crate::application::catalog::ToolCatalog;
use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use regent_store::Store;
use serde_json::{Value, json};
use std::sync::Arc;

pub fn register_persona_tool(
    catalog: &mut ToolCatalog,
    store: Arc<Store>,
) -> Result<(), RegentError> {
    catalog.register(persona_definition(), Arc::new(PersonaTool { store }))
}

fn persona_definition() -> ToolDefinition {
    ToolDefinition {
        name: "update_persona".into(),
        description: "Edit your own persona (target 'self' = your identity/tone/behaviour) or the \
                      user's stable profile (target 'user'). The profile holds ONLY durable facts \
                      about the person, split into five sections — pass `section`: 'identity' \
                      (name, role, location), 'preferences' (how they like answers/tools), 'habits' \
                      (recurring behaviour), 'constraints' (OS, tooling, hard limits), 'goals' \
                      (what they're building). Do NOT put transient state here — a current download, \
                      today's task, a one-off path belong in the `memory` tool (world/work facts) \
                      or just stay in the conversation; what happened is already in session \
                      history. action 'set' replaces, 'append' adds a line, 'get' reads. Changes \
                      take full effect next session (/new) — acknowledge the change."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "target": {"type": "string", "enum": ["self", "user"]},
                "section": {
                    "type": "string",
                    "enum": ["identity", "preferences", "habits", "constraints", "goals"],
                    "description": "Required for target 'user': which profile facet to edit."
                },
                "action": {"type": "string", "enum": ["set", "append", "get"]},
                "text": {"type": "string", "description": "Content for set/append."}
            },
            "required": ["target", "action"]
        }),
        toolset: "persona".into(),
    }
}

struct PersonaTool {
    store: Arc<Store>,
}

#[async_trait]
impl ToolExecutor for PersonaTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let store = Arc::clone(&self.store);
        tokio::task::spawn_blocking(move || Ok(run_persona_action(&store, &args)))
            .await
            .map_err(|e| RegentError::Tool {
                tool: "update_persona".into(),
                message: e.to_string(),
            })?
    }
}

fn run_persona_action(store: &Store, args: &Value) -> String {
    let section = args.get("section").and_then(Value::as_str);
    let key: String = match args.get("target").and_then(Value::as_str) {
        Some("self") => "soul".into(),
        // target 'user' writes a profile facet (about.<section>); bare 'about'
        // stays a back-compat catch-all when no section is given.
        Some("user") => match section {
            Some(s) if regent_store::is_valid_persona_key(&format!("about.{s}")) => {
                format!("about.{s}")
            }
            Some(s) => return tool_error_json(format!("unknown profile section '{s}'")),
            None => "about".into(),
        },
        _ => return tool_error_json("target must be 'self' or 'user'"),
    };
    let key = key.as_str();
    let action = args.get("action").and_then(Value::as_str).unwrap_or("get");
    let text = args.get("text").and_then(Value::as_str).unwrap_or("");
    let result: Result<Value, String> = match action {
        "get" => store
            .get_persona(key)
            .map(|c| json!({ "content": c }))
            .map_err(|e| e.to_string()),
        "set" => store
            .set_persona(key, text)
            .map(|()| json!({ "success": true }))
            .map_err(|e| e.to_string()),
        "append" => match store.get_persona(key) {
            Ok(cur) => {
                let next = if cur.trim().is_empty() {
                    text.to_owned()
                } else {
                    format!("{}\n{text}", cur.trim_end())
                };
                store
                    .set_persona(key, &next)
                    .map(|()| json!({ "success": true }))
                    .map_err(|e| e.to_string())
            }
            Err(e) => Err(e.to_string()),
        },
        other => return tool_error_json(format!("unknown action '{other}'")),
    };
    match result {
        Ok(v) => v.to_string(),
        Err(m) => tool_error_json(m),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_append_get_roundtrip() {
        let store = Store::open_in_memory().unwrap();
        let set = run_persona_action(
            &store,
            &json!({"target": "self", "action": "set", "text": "You are Jepitot."}),
        );
        assert!(set.contains("\"success\":true"));
        run_persona_action(
            &store,
            &json!({"target": "self", "action": "append", "text": "Be witty."}),
        );
        let got = run_persona_action(&store, &json!({"target": "self", "action": "get"}));
        assert!(got.contains("Jepitot"));
        assert!(got.contains("Be witty."));
    }

    #[test]
    fn bad_target_is_a_tool_error() {
        let store = Store::open_in_memory().unwrap();
        assert!(
            run_persona_action(&store, &json!({"target": "x", "action": "get"})).contains("error")
        );
    }

    #[test]
    fn user_section_writes_about_facet() {
        let store = Store::open_in_memory().unwrap();
        let set = run_persona_action(
            &store,
            &json!({"target": "user", "section": "goals", "action": "set", "text": "Ship local voice"}),
        );
        assert!(set.contains("\"success\":true"));
        // It lands under the about.goals key, not the legacy `about` blob.
        assert_eq!(
            store.get_persona("about.goals").unwrap(),
            "Ship local voice"
        );
        assert_eq!(store.get_persona("about").unwrap(), "");
        // And it renders into the profile block as a Goals facet.
        assert!(store.persona_block().contains("### Goals"));
    }

    #[test]
    fn unknown_section_is_a_tool_error() {
        let store = Store::open_in_memory().unwrap();
        let out = run_persona_action(
            &store,
            &json!({"target": "user", "section": "salary", "action": "set", "text": "x"}),
        );
        assert!(out.contains("error"));
    }
}
