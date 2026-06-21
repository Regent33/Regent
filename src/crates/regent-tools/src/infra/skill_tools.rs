//! Skill-facing core tools (procedural memory): `skills_list` (level-0
//! index), `skill_view` (full content or one reference file — deliberately
//! NO pagination: models read page 1 and stop), and `skill_manage`
//! (create / patch / archive; archive is the maximum destructive action).

use crate::application::catalog::ToolCatalog;
use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use regent_skills::SkillLibrary;
use serde_json::{Value, json};
use std::sync::Arc;

pub fn register_skill_tools(
    catalog: &mut ToolCatalog,
    library: Arc<SkillLibrary>,
) -> Result<(), RegentError> {
    catalog.register(
        list_definition(),
        Arc::new(SkillsListTool {
            library: Arc::clone(&library),
        }),
    )?;
    catalog.register(
        view_definition(),
        Arc::new(SkillViewTool {
            library: Arc::clone(&library),
        }),
    )?;
    catalog.register(manage_definition(), Arc::new(SkillManageTool { library }))?;
    Ok(())
}

fn list_definition() -> ToolDefinition {
    ToolDefinition {
        name: "skills_list".into(),
        description: "List available skills (name + description). Load a matching one with \
                      skill_view before doing the task it covers."
            .into(),
        parameters: json!({"type": "object", "properties": {}}),
        toolset: "skills".into(),
    }
}

fn view_definition() -> ToolDefinition {
    ToolDefinition {
        name: "skill_view".into(),
        description: "Load a skill's full instructions (read it completely), or one of its \
                      reference files via path."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "path": {"type": "string", "description": "Optional reference file inside the skill."}
            },
            "required": ["name"]
        }),
        toolset: "skills".into(),
    }
}

fn manage_definition() -> ToolDefinition {
    ToolDefinition {
        name: "skill_manage".into(),
        description: "Maintain the skill library: create (name, description ≤60 chars ending \
                      with a period, body), patch (old_text→new_text, unique match), or archive. \
                      Prefer patching an existing skill over creating a narrow new one."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "action": {"type": "string", "enum": ["create", "patch", "archive"]},
                "name": {"type": "string"},
                "description": {"type": "string"},
                "body": {"type": "string"},
                "old_text": {"type": "string"},
                "new_text": {"type": "string"}
            },
            "required": ["action", "name"]
        }),
        toolset: "skills".into(),
    }
}

struct SkillsListTool {
    library: Arc<SkillLibrary>,
}

#[async_trait]
impl ToolExecutor for SkillsListTool {
    async fn execute(&self, _args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let library = Arc::clone(&self.library);
        bridge("skills_list", move || match library.list() {
            Ok(summaries) => {
                let skills: Vec<Value> = summaries
                    .iter()
                    .map(|s| json!({"name": s.name, "description": s.description, "tags": s.tags}))
                    .collect();
                json!({"skills": skills, "count": skills.len()}).to_string()
            }
            Err(error) => tool_error_json(error.to_string()),
        })
        .await
    }
}

struct SkillViewTool {
    library: Arc<SkillLibrary>,
}

#[async_trait]
impl ToolExecutor for SkillViewTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(name) = args
            .get("name")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
        else {
            return Ok(tool_error_json("missing required parameter: name"));
        };
        let path = args
            .get("path")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let library = Arc::clone(&self.library);
        bridge("skill_view", move || match path {
            Some(relative) => match library.view_file(&name, &relative) {
                Ok(content) => {
                    json!({"name": name, "path": relative, "content": content}).to_string()
                }
                Err(error) => tool_error_json(error.to_string()),
            },
            None => match library.view(&name) {
                Ok(record) => json!({
                    "name": record.meta.name,
                    "description": record.meta.description,
                    "version": record.meta.version,
                    "body": record.body,
                    "files": record.files,
                })
                .to_string(),
                Err(error) => tool_error_json(error.to_string()),
            },
        })
        .await
    }
}

struct SkillManageTool {
    library: Arc<SkillLibrary>,
}

#[async_trait]
impl ToolExecutor for SkillManageTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let library = Arc::clone(&self.library);
        bridge("skill_manage", move || run_manage(&library, &args)).await
    }
}

fn run_manage(library: &SkillLibrary, args: &Value) -> String {
    let action = args
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let Some(name) = args.get("name").and_then(Value::as_str) else {
        return tool_error_json("missing required parameter: name");
    };
    let text = |key: &str| args.get(key).and_then(Value::as_str);
    let outcome = match action {
        "create" => match (text("description"), text("body")) {
            (Some(description), Some(body)) => library
                .create(name, description, body, "agent")
                .map(|()| "created"),
            _ => return tool_error_json("create needs description and body"),
        },
        "patch" => match (text("old_text"), text("new_text")) {
            (Some(old_text), Some(new_text)) => {
                library.patch(name, old_text, new_text).map(|()| "patched")
            }
            _ => return tool_error_json("patch needs old_text and new_text"),
        },
        "archive" => library.archive(name).map(|()| "archived"),
        other => return tool_error_json(format!("unknown action '{other}'")),
    };
    match outcome {
        Ok(message) => json!({"success": true, "result": message, "name": name}).to_string(),
        Err(error) => tool_error_json(error.to_string()),
    }
}

/// Library calls are blocking filesystem I/O — bridged off the runtime.
async fn bridge(
    tool: &'static str,
    work: impl FnOnce() -> String + Send + 'static,
) -> Result<String, RegentError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| RegentError::Tool {
            tool: tool.into(),
            message: e.to_string(),
        })
}
