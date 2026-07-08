//! The `regent` tool — lets the agent run its OWN admin commands in-process.
//!
//! The agent IS the deacon, so shelling out to the `regent` CLI would spawn a
//! second deacon that deadlocks on the shared store (see the terminal tool's
//! short-circuit). Instead this tool forwards a method + params straight to the
//! deacon's existing JSON-RPC dispatcher — the SAME handlers the CLI drives — so
//! "set my model", "what's my status", "schedule a cron" actually run, with no
//! second process and no command-mapping duplication.

use crate::application::session_manager::SessionManager;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use regent_tools::{ToolContext, ToolExecutor};
use serde_json::{Value, json};
use std::sync::Weak;

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "regent".into(),
        description: "Run one of Regent's OWN admin commands in-process (you are the deacon — never \
             use the terminal for `regent ...`, it deadlocks). Give `method` (a deacon RPC method) \
             and `params`. Common: status.get{} · model.get{} · model.list{} · model.set{id} · \
             config.get{} · config.set{path,value} · insights.get{} · skills.list{} · \
             skills.create{name,description,body} · \
             agents.list{} · agents.set{name,role,prompt,...} · providers.list{} · \
             providers.test{name} · mom.run{name,brief} · cron.list{} · cron.add{...} · \
             voice.status{} · voice.models{} · voice.set{asr_model?,tts_model?,whisper_size?,\
             vision_model?,vision_base_url?} (change your own speech/vision models yourself — \
             applies on the next voice-server/deacon start, say so) · tools.list{} · \
             commands.list{}. To change config.yaml (default provider/model, context size), ALWAYS \
             use config.set{path,value} — NEVER hand-edit config.yaml with file_edit/terminal: \
             config.set validates the whole file against the real schema before writing, so an \
             invalid provider or typo is rejected instead of bricking the next start. `providers` \
             is a NAME-KEYED MAP (not a list): each entry is {kind, api_key_env, models:[…], \
             base_url?} where kind is one of anthropic·openai·openrouter·groq·deepseek·together·\
             ollama·mistral·xai·gemini·moonshot·zhipu·dashscope·fireworks·cerebras·perplexity·\
             minimax (kind alone resolves the right base URL + key convention — only set base_url \
             for a non-standard host). Set keys with the manage_keys tool FIRST, then reference \
             the env var. Examples: config.set{path:'providers.groq', value:{kind:'groq', \
             api_key_env:'GROQ_API_KEY', models:['llama-3.3-70b-versatile']}} · \
             config.set{path:'agents_defaults.primary', value:{provider:'groq', \
             model:'llama-3.3-70b-versatile'}} (fallback chain: agents_defaults.fallbacks, an \
             ordered list of the same {provider,model} shape) · \
             config.set{path:'context.max_tokens', value:120000}. A missing \
             param comes back as a clear \
             error naming it. Commands with NO deacon method (gateway, setup, doctor, \
             providers add/remove, agents mom create/remove, keys — use the manage_keys tool, auth, \
             security, debug, mcp, logs) can't run here: tell the user the exact `regent <command>` to run."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "method": {"type": "string", "description": "Deacon RPC method, e.g. 'model.set', 'status.get', 'cron.add'."},
                "params": {"type": "object", "description": "Arguments for the method (default {})."}
            },
            "required": ["method"]
        }),
        toolset: "regent".into(),
    }
}

/// Forwards admin commands to the live `SessionManager`'s dispatcher. Holds a
/// `Weak` so the tool never keeps the manager alive past shutdown.
pub struct RegentCommandTool {
    sessions: Weak<SessionManager>,
}

impl RegentCommandTool {
    #[must_use]
    pub fn new(sessions: Weak<SessionManager>) -> Self {
        Self { sessions }
    }
}

#[async_trait]
impl ToolExecutor for RegentCommandTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(method) = args.get("method").and_then(Value::as_str) else {
            return Ok(tool_error_json("missing required parameter: method"));
        };
        let params = args.get("params").cloned().unwrap_or_else(|| json!({}));
        let Some(sessions) = self.sessions.upgrade() else {
            return Ok(tool_error_json("deacon is shutting down"));
        };
        match sessions.run_admin_command(method, params).await {
            Ok(result) => Ok(result.to_string()),
            Err(message) => Ok(tool_error_json(message)),
        }
    }
}
