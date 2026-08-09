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
use std::collections::HashSet;
use std::sync::Arc;

pub fn register_persona_tool(
    catalog: &mut ToolCatalog,
    store: Arc<Store>,
) -> Result<(), RegentError> {
    catalog.register(persona_definition(), Arc::new(PersonaTool { store }))
}

/// Background-review persona writes are deliberately narrower than live-chat
/// writes: only append durable user-profile facts. The reviewer cannot replace
/// a profile section or change Regent's own identity from transcript text.
pub fn register_review_persona_tool(
    catalog: &mut ToolCatalog,
    store: Arc<Store>,
) -> Result<(), RegentError> {
    let mut definition = persona_definition();
    definition.description = "Append a durable fact or preference to one user profile section. Background review cannot replace profile data or change Regent's own persona.".into();
    definition.parameters["properties"]["target"]["enum"] = json!(["user"]);
    definition.parameters["properties"]["action"]["enum"] = json!(["append"]);
    catalog.register(definition, Arc::new(ReviewPersonaTool { store }))
}

fn persona_definition() -> ToolDefinition {
    ToolDefinition {
        name: "update_persona".into(),
        description: "Edit your own persona (target 'self' ONLY for Regent's explicit name, identity, \
                      or core persona) or the user's stable profile (target 'user'). Preferences \
                      about how the user wants answers, tools, tone, formatting, or workflow ALWAYS \
                      use target 'user', section 'preferences'; they are not Regent's own persona. \
                      The profile holds ONLY durable facts \
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

struct ReviewPersonaTool {
    store: Arc<Store>,
}

#[async_trait]
impl ToolExecutor for ReviewPersonaTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        if args.get("target").and_then(Value::as_str) != Some("user")
            || args.get("action").and_then(Value::as_str) != Some("append")
        {
            return Ok(tool_error_json(
                "background review may only append to the user profile; self/set/get are denied",
            ));
        }
        let store = Arc::clone(&self.store);
        tokio::task::spawn_blocking(move || Ok(run_persona_action(&store, &args)))
            .await
            .map_err(|e| RegentError::Tool {
                tool: "update_persona".into(),
                message: e.to_string(),
            })?
    }
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
        // A user-profile write must name its facet. The old fallback to bare
        // `about` made reviewers read/write the wrong row while claiming the
        // structured profile had been updated.
        Some("user") => match section {
            Some(s) if regent_store::is_valid_persona_key(&format!("about.{s}")) => {
                format!("about.{s}")
            }
            Some(s) => return tool_error_json(format!("unknown profile section '{s}'")),
            None => return tool_error_json("section is required when target is 'user'"),
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
                let mut seen: HashSet<String> = cur.lines().map(normalize_line).collect();
                let additions: Vec<&str> = text
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .filter(|line| seen.insert(normalize_line(line)))
                    .collect();
                if additions.is_empty() {
                    Ok(json!({ "success": true, "changed": false }))
                } else {
                    let separator = if cur.trim().is_empty() { "" } else { "\n" };
                    let next = format!("{}{separator}{}", cur.trim_end(), additions.join("\n"));
                    store
                        .set_persona(key, &next)
                        .map(|()| json!({ "success": true, "changed": true }))
                        .map_err(|e| e.to_string())
                }
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

fn normalize_line(line: &str) -> String {
    line.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
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

    #[test]
    fn user_target_requires_a_facet() {
        let store = Store::open_in_memory().unwrap();
        let out = run_persona_action(
            &store,
            &json!({"target": "user", "action": "append", "text": "Rainer"}),
        );
        assert!(out.contains("section is required"));
        assert_eq!(store.get_persona("about").unwrap(), "");
    }

    #[test]
    fn append_is_idempotent_across_case_and_whitespace() {
        let store = Store::open_in_memory().unwrap();
        let args = json!({
            "target": "user",
            "section": "identity",
            "action": "append",
            "text": "Name is Rainer aka regent33"
        });
        assert!(run_persona_action(&store, &args).contains("\"changed\":true"));
        let duplicate = json!({
            "target": "user",
            "section": "identity",
            "action": "append",
            "text": "  NAME   is rainer AKA regent33  "
        });
        assert!(run_persona_action(&store, &duplicate).contains("\"changed\":false"));
        assert_eq!(
            store.get_persona("about.identity").unwrap().lines().count(),
            1
        );
    }
}
