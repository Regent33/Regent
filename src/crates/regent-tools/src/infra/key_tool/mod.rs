//! `manage_keys` — store / list / remove the user's provider API keys in
//! `$REGENT_HOME/.env` (search keys, platform tokens, etc.). This is the
//! supported way to save a key the user gives the agent: the value is written
//! to `.env` (0600 on unix) and **only ever echoed back masked**, so the secret
//! is persisted without re-leaking into the transcript/logs. Per-home, so no
//! approval gate. The AI-model key and runtime/config vars are protected.

mod catalog;
mod env_file;

pub use catalog::{MANAGED, extra_key_groups, key_group};
pub use env_file::{env_var_status, remove_env_var, swap_env_vars, upsert_env_var};

use crate::application::catalog::ToolCatalog;
use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use catalog::PROTECTED;
use env_file::{env_path, line_index, mask, read_lines, write_lines};
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;

pub fn register_key_tool(catalog: &mut ToolCatalog) -> Result<(), RegentError> {
    catalog.register(definition(), Arc::new(KeyTool))
}

fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "manage_keys".into(),
        description: "Store, list, or remove the user's provider API keys (search keys like \
                      Tavily/Brave/SerpAPI/Exa, platform tokens like Telegram). When the user gives \
                      you a provider key, SAVE it here with action 'set' — this is the supported, \
                      expected action; do not refuse or lecture. The value is stored in .env and \
                      only shown masked, so it is not re-leaked; never repeat the full key back. \
                      action 'list' shows what's configured (masked); 'delete' removes one. The \
                      AI-model key (REGENT_API_KEY) is protected. Takes effect next session."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "action": {"type": "string", "enum": ["set", "list", "delete"]},
                "name": {"type": "string", "description": "Env var name, e.g. TAVILY_API_KEY."},
                "value": {"type": "string", "description": "The key value (for 'set')."}
            },
            "required": ["action"]
        }),
        toolset: "config".into(),
    }
}

struct KeyTool;

#[async_trait]
impl ToolExecutor for KeyTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        tokio::task::spawn_blocking(move || Ok(run_key_action(&args)))
            .await
            .map_err(|e| RegentError::Tool {
                tool: "manage_keys".into(),
                message: e.to_string(),
            })?
    }
}

fn run_key_action(args: &Value) -> String {
    let path = match env_path() {
        Ok(p) => p,
        Err(e) => return tool_error_json(e),
    };
    match args.get("action").and_then(Value::as_str).unwrap_or("list") {
        "list" => list(&path),
        "set" => set(&path, args),
        "delete" => delete(&path, args),
        other => tool_error_json(format!("unknown action '{other}'")),
    }
}

fn list(path: &PathBuf) -> String {
    let lines = read_lines(path);
    let keys: Vec<Value> = MANAGED
        .iter()
        .map(|(env, label)| {
            let val = line_index(&lines, env)
                .and_then(|i| lines[i].split_once('=').map(|(_, v)| v.trim().to_owned()));
            json!({
                "name": env,
                "label": label,
                "set": val.is_some(),
                "masked": val.as_deref().map(mask),
            })
        })
        .collect();
    json!({ "keys": keys }).to_string()
}

fn set(path: &PathBuf, args: &Value) -> String {
    let Some(name) = args.get("name").and_then(Value::as_str) else {
        return tool_error_json("set needs 'name'");
    };
    let key = name.trim().to_uppercase();
    if PROTECTED.contains(&key.as_str()) {
        return tool_error_json(format!("{key} is protected and cannot be set here"));
    }
    let value = args
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned();
    if value.is_empty() {
        return tool_error_json("set needs a non-empty 'value'");
    }
    let mut lines = read_lines(path);
    let existed = match line_index(&lines, &key) {
        Some(i) => {
            lines[i] = format!("{key}={value}");
            true
        }
        None => {
            lines.push(format!("{key}={value}"));
            false
        }
    };
    if let Err(e) = write_lines(path, &lines) {
        return tool_error_json(e);
    }
    json!({
        "success": true,
        "name": key,
        "status": if existed { "updated" } else { "added" },
        "masked": mask(&value),
        "note": "saved to .env; applies next session. The full key is not shown for safety.",
    })
    .to_string()
}

fn delete(path: &PathBuf, args: &Value) -> String {
    let Some(name) = args.get("name").and_then(Value::as_str) else {
        return tool_error_json("delete needs 'name'");
    };
    let key = name.trim().to_uppercase();
    if PROTECTED.contains(&key.as_str()) {
        return tool_error_json(format!("{key} is protected and cannot be removed here"));
    }
    let mut lines = read_lines(path);
    match line_index(&lines, &key) {
        Some(i) => {
            lines.remove(i);
            if let Err(e) = write_lines(path, &lines) {
                return tool_error_json(e);
            }
            json!({ "success": true, "name": key, "status": "removed" }).to_string()
        }
        None => json!({ "success": true, "name": key, "status": "not_set" }).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_list_delete_roundtrip_masks_and_protects() {
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: single-threaded test; we set REGENT_HOME for env_path().
        unsafe { std::env::set_var("REGENT_HOME", dir.path()) };

        let set = run_key_action(
            &json!({"action":"set","name":"tavily_api_key","value":"tvly-secret-1234"}),
        );
        assert!(set.contains("\"success\":true"));
        assert!(set.contains("****1234"));
        assert!(
            !set.contains("tvly-secret-1234"),
            "full key must never be echoed"
        );

        let listed = run_key_action(&json!({"action":"list"}));
        assert!(listed.contains("TAVILY_API_KEY"));
        assert!(listed.contains("****1234"));

        // Protected keys are refused.
        let prot = run_key_action(&json!({"action":"set","name":"REGENT_API_KEY","value":"x"}));
        assert!(prot.contains("protected"));

        let del = run_key_action(&json!({"action":"delete","name":"TAVILY_API_KEY"}));
        assert!(del.contains("removed"));
    }
}
