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
        // Description kept lean: the provider-setup rationale, the config.set
        // JSON examples, and the "commands with no deacon method" list all live
        // verbatim in the always-resident CAPABILITIES prompt, so repeating them
        // here just doubled the single biggest RESIDENT tool schema (~720 tok)
        // every full/voice turn. Unique-to-here info stays: the in-process rule,
        // the method menu, the provider-`kind` enum, and the gateway exception.
        description: "Run one of Regent's OWN admin commands in-process (you are the deacon — never \
             use the terminal for `regent ...`, it deadlocks). Give `method` (a deacon RPC method) \
             and `params`. Common: status.get{} · model.get{} · model.list{} · model.set{id} · \
             config.get{} · config.set{path,value} · insights.get{} · skills.list{} · \
             skills.create{name,description,body} · \
             agents.list{} · agents.set{name,description,system_prompt,model?,tools?} (create or \
             update a named agent) · agents.remove{name} · providers.list{} · \
             providers.test{name} · mom.run{name,brief} · cron.list{} · cron.add{...} · \
             voice.status{} · voice.models{} · voice.set{asr_model?,tts_model?,whisper_size?,\
             whisper_lang?,vision_model?,vision_base_url?} (change your own speech/vision models yourself — \
             applies on the next voice-server/deacon start, say so) · tools.list{} · \
             commands.list{}. For ANY config change (default provider/model, context size, adding a \
             provider), use config.set{path,value} — NEVER hand-edit config.yaml with \
             file_edit/terminal; your command reference has the exact two-step provider setup. \
             `providers` is a NAME-KEYED MAP (not a list): each entry is {kind, api_key_env, \
             models:[…], base_url?} where kind is one of anthropic·openai·openrouter·groq·deepseek·\
             together·ollama·mistral·xai·gemini·moonshot·zhipu·dashscope·fireworks·cerebras·\
             perplexity·minimax (kind alone resolves the right base URL + key convention — only set \
             base_url for a non-standard host); save the key with the manage_keys tool FIRST, then \
             reference its env var. A missing param comes back as a clear error naming it. Commands \
             with NO deacon method (setup, doctor, keys, auth, security, debug, mcp, logs, providers \
             remove) can't run here: tell the user the exact `regent <command>` to run. The ONE \
             exception you can run yourself is `regent gateway setup|start|stop|status` — through \
             the terminal tool, since it needs no deacon."
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

#[cfg(test)]
mod tests {
    use super::definition;
    use regent_agent::CAPABILITIES;

    /// The `regent` tool is the single biggest RESIDENT tool schema on the full
    /// profile. Its unique contract must survive the token-dedup: the
    /// in-process/deadlock rule, the config.set write-path rule, the full
    /// provider-`kind` enum (found nowhere else), and the gateway exception.
    #[test]
    fn regent_tool_keeps_its_unique_contract() {
        let d = definition().description;
        assert!(d.contains("deadlocks"), "in-process/deadlock rule dropped");
        assert!(d.contains("config.set"), "config write-path rule dropped");
        assert!(d.contains("NAME-KEYED MAP"), "providers shape dropped");
        // The provider-`kind` enum lives ONLY here — losing it would make the
        // model guess kinds. Spot-check both ends of the list.
        assert!(d.contains("anthropic·openai"), "kind enum head dropped");
        assert!(d.contains("minimax"), "kind enum tail dropped");
        assert!(
            d.contains("gateway setup|start|stop|status"),
            "gateway exception dropped",
        );
    }

    /// Dedup proof: the provider-setup RATIONALE and the concrete config.set
    /// EXAMPLES were removed from this tool because they are already carried,
    /// verbatim-equivalent, by the always-resident CAPABILITIES prompt. Assert
    /// they are gone HERE and still present THERE — the contract moved, it
    /// wasn't lost, and the duplicate no longer rides every full/voice turn.
    #[test]
    fn provider_setup_detail_is_not_duplicated_from_capabilities() {
        let d = definition().description;
        // The verbose validation rationale + the worked groq example are the
        // duplicated spans; neither should remain in the tool description.
        assert!(
            !d.contains("validates the whole file"),
            "validation rationale still duplicated here — it belongs to CAPABILITIES",
        );
        assert!(
            !d.contains("providers.groq"),
            "the config.set example still duplicated here — it belongs to CAPABILITIES",
        );
        // …and the contract is still resident somewhere every turn.
        assert!(
            CAPABILITIES.contains("config.set validates the whole file"),
            "CAPABILITIES must still carry the config.set validation contract",
        );
        assert!(
            CAPABILITIES.contains("save the key with manage_keys"),
            "CAPABILITIES must still carry the provider key-setup steps",
        );
    }
}
