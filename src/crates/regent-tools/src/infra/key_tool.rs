//! `manage_keys` — store / list / remove the user's provider API keys in
//! `$REGENT_HOME/.env` (search keys, platform tokens, etc.). This is the
//! supported way to save a key the user gives the agent: the value is written
//! to `.env` (0600 on unix) and **only ever echoed back masked**, so the secret
//! is persisted without re-leaking into the transcript/logs. Per-home, so no
//! approval gate. The AI-model key and runtime/config vars are protected.

use crate::application::catalog::ToolCatalog;
use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;

/// Provider keys the tool advertises in `list` (others are still settable).
const MANAGED: &[(&str, &str)] = &[
    (
        "REGENT_SEARCH_PROVIDER",
        "search provider (brave|tavily|serpapi|exa|google_cse|duckduckgo)",
    ),
    ("REGENT_SEARCH_API_KEY", "search key (generic fallback)"),
    ("BRAVE_API_KEY", "Brave Search key"),
    ("TAVILY_API_KEY", "Tavily key"),
    ("SERPAPI_API_KEY", "SerpAPI key"),
    ("EXA_API_KEY", "Exa key"),
    ("GOOGLE_CSE_API_KEY", "Google CSE key"),
    ("GOOGLE_CSE_CX", "Google CSE engine id (cx)"),
    ("REGENT_TELEGRAM_TOKEN", "Telegram bot token"),
    ("REGENT_TELEGRAM_ALLOWED_USERS", "Telegram allowed user ids (comma-sep)"),
    ("REGENT_DISCORD_TOKEN", "Discord bot token"),
    ("DISCORD_PUBLIC_KEY", "Discord interactions public key"),
    ("SLACK_BOT_TOKEN", "Slack bot token"),
    ("SLACK_SIGNING_SECRET", "Slack signing secret"),
    ("WHATSAPP_ACCESS_TOKEN", "WhatsApp access token"),
    ("WHATSAPP_APP_SECRET", "WhatsApp app secret"),
    ("WHATSAPP_PHONE_NUMBER_ID", "WhatsApp phone number id"),
    ("MESSENGER_PAGE_TOKEN", "Messenger page token"),
    ("MESSENGER_APP_SECRET", "Messenger app secret"),
    ("LINE_CHANNEL_ACCESS_TOKEN", "LINE channel access token"),
    ("LINE_CHANNEL_SECRET", "LINE channel secret"),
    ("MATTERMOST_URL", "Mattermost server URL"),
    ("MATTERMOST_BOT_TOKEN", "Mattermost bot token"),
    ("MATTERMOST_VERIFY_TOKEN", "Mattermost outgoing-webhook verify token"),
    ("TWILIO_ACCOUNT_SID", "Twilio account SID"),
    ("TWILIO_AUTH_TOKEN", "Twilio auth token"),
    ("TWILIO_FROM_NUMBER", "Twilio from number"),
    ("TEAMS_OUTGOING_SECRET", "Teams outgoing-webhook secret"),
    ("FEISHU_VERIFICATION_TOKEN", "Feishu verification token"),
    ("FEISHU_ENCRYPT_KEY", "Feishu encrypt key"),
    ("FEISHU_TENANT_TOKEN", "Feishu tenant access token"),
    ("WECHAT_TOKEN", "WeChat token"),
    ("WECHAT_ENCODING_AES_KEY", "WeChat encoding AES key"),
    ("WECHAT_ACCESS_TOKEN", "WeChat access token"),
    ("WECOM_TOKEN", "WeCom token"),
    ("WECOM_ENCODING_AES_KEY", "WeCom encoding AES key"),
    ("WECOM_ACCESS_TOKEN", "WeCom access token"),
    ("WECOM_AGENT_ID", "WeCom agent id"),
    ("MAILGUN_API_KEY", "Mailgun API key"),
    ("MAILGUN_SIGNING_KEY", "Mailgun webhook signing key"),
    ("MAILGUN_DOMAIN", "Mailgun domain"),
    ("MAILGUN_FROM", "Mailgun from address"),
    ("JIRA_EMAIL", "Jira account email"),
    ("JIRA_API_TOKEN", "Jira API token"),
    ("JIRA_BASE_URL", "Jira base URL"),
    ("JIRA_WEBHOOK_SECRET", "Jira webhook secret"),
    ("AZURE_DEVOPS_PAT", "Azure DevOps PAT"),
    ("AZURE_DEVOPS_ORG_URL", "Azure DevOps org URL"),
    ("TRELLO_API_KEY", "Trello API key"),
    ("TRELLO_API_SECRET", "Trello API secret"),
    ("TRELLO_TOKEN", "Trello token"),
    ("GCHAT_AUDIENCE", "Google Chat audience (project number)"),
    ("REGENT_SPEECH_PROVIDER", "speech provider (for voice calls)"),
    ("REGENT_SPEECH_API_KEY", "speech API key (for voice calls)"),
];

/// Never writable here: the AI-model secret + runtime/config vars (avoid the
/// agent clobbering its own model/provider wiring through this tool).
const PROTECTED: &[&str] = &[
    "REGENT_API_KEY",
    "REGENT_MODEL",
    "REGENT_BASE_URL",
    "REGENT_PROVIDER",
    "REGENT_HOME",
    "REGENT_NOW",
];

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

fn env_path() -> Result<PathBuf, String> {
    let home = std::env::var("REGENT_HOME").map_err(|_| "REGENT_HOME is not set".to_owned())?;
    Ok(PathBuf::from(home).join(".env"))
}

fn read_lines(path: &PathBuf) -> Vec<String> {
    std::fs::read_to_string(path)
        .map(|s| s.lines().map(str::to_owned).collect())
        .unwrap_or_default()
}

fn write_lines(path: &PathBuf, lines: &[String]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let body = format!("{}\n", lines.join("\n"));
    std::fs::write(path, body).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(windows)]
    {
        // The 0600 equivalent: strip inherited ACEs, grant only the current
        // user. Best-effort, same as the unix branch.
        let user = std::env::var("USERNAME").unwrap_or_default();
        if !user.is_empty() {
            let _ = std::process::Command::new("icacls")
                .arg(path)
                .args(["/inheritance:r", "/grant:r", &format!("{user}:F")])
                .output();
        }
    }
    Ok(())
}

fn line_index(lines: &[String], key: &str) -> Option<usize> {
    lines
        .iter()
        .position(|l| l.trim_start().starts_with(&format!("{key}=")))
}

fn mask(v: &str) -> String {
    let t = v.trim();
    if t.len() <= 4 {
        "****".into()
    } else {
        format!("****{}", &t[t.len() - 4..])
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
