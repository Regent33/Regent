//! `ask_user` (gap T4) — one blocking question to the human. Rides the existing
//! approval channel end-to-end: the surface renders the question, the reply
//! comes back as the approval decision (`Approve` = plain yes, the deny-feedback
//! string = a free-text answer). No new RPC method needed for the plain form.
//!
//! Passing `questions` upgrades that to a real form — single-select,
//! multi-select, confirm, rank — rendered as a card by surfaces that can draw
//! one and as numbered text by every surface that cannot. The answer comes back
//! typed either way, so the model never re-parses a sentence it wrote itself.

use crate::ToolCatalog;
use crate::domain::contracts::{ApprovalDecision, ToolExecutor};
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::contracts::questionnaire::{
    self, Answer, Question, Questionnaire, QuestionnaireAnswer,
};
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use serde_json::{Value, json};
use std::sync::Arc;

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "ask_user".into(),
        description: concat!(
            "Ask the user a question and wait for their answer. Use it when you cannot ",
            "proceed safely on assumptions (missing requirement, ambiguous instruction, ",
            "irreversible choice) - AND whenever they ask to be asked: a quiz, a poll, a ",
            "questionnaire, \"ask me N questions\", or any request to pick between options. ",
            "This IS the questionnaire UI; never hand-roll one by writing an HTML file or ",
            "printing a numbered list, since neither can collect an answer. Prefer ",
            "`questions` - offering concrete options gets a decision instead of a ",
            "paragraph. Never write your own 'Other'/'Something else' option: a free-text ",
            "row is always added for you.",
        )
        .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "question": {
                    "type": "string",
                    "description": "The question, self-contained. Used on its own for an \
                         open question, or as the intro line above `questions`."
                },
                "context": {
                    "type": "string",
                    "description": "Why you're asking, in one or two lines."
                },
                "questions": {
                    "type": "array",
                    "description": "1-5 structured questions. Each is rendered as a real \
                         form control the user picks from.",
                    "maxItems": questionnaire::MAX_QUESTIONS,
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": {
                                "type": "string",
                                "description": "Short unique key you'll read the answer back by."
                            },
                            "prompt": {"type": "string", "description": "The question itself."},
                            "header": {
                                "type": "string",
                                "description": "1-2 word chip label ('Auth method', 'Scope')."
                            },
                            "kind": {
                                "type": "string",
                                "enum": [
                                    "single_select", "multi_select", "text", "confirm", "rank"
                                ],
                                "description": "single_select picks one; multi_select picks \
                                     any; rank orders them by pick order; confirm is yes/no; \
                                     text is open. Select kinds need 2-6 options; confirm \
                                     and text take none."
                            },
                            "options": {
                                "type": "array",
                                "maxItems": questionnaire::MAX_OPTIONS,
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "id": {"type": "string"},
                                        "label": {
                                            "type": "string",
                                            "description": "Short — a few words."
                                        },
                                        "description": {
                                            "type": "string",
                                            "description": "One line carrying the real \
                                                 trade-off behind this choice."
                                        }
                                    },
                                    "required": ["id", "label"]
                                }
                            },
                            "required": {
                                "type": "boolean",
                                "description": "True if the user should not skip this one."
                            }
                        },
                        "required": ["id", "prompt", "kind"]
                    }
                }
            },
            "required": ["question"]
        }),
        toolset: "core".into(),
    }
}

struct AskUserTool;

#[async_trait]
impl ToolExecutor for AskUserTool {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(question) = args.get("question").and_then(Value::as_str) else {
            return Ok(tool_error_json("ask_user needs 'question' (a string)"));
        };
        let context = args
            .get("context")
            .and_then(Value::as_str)
            .unwrap_or_default();

        match structured(question, &args) {
            // A validation failure goes back to the model, never to the human:
            // a card no surface could render must not reach a person.
            Err(message) => Ok(tool_error_json(&message)),
            Ok(None) => plain(question, context, ctx).await,
            Ok(Some(questionnaire)) => {
                let answer = ctx.approval.request_structured(&questionnaire).await;
                Ok(render_answer(&questionnaire, &answer))
            }
        }
    }
}

/// The open-question path — unchanged behavior for a model that sends no
/// `questions`, and the reason a plain `ask_user` still needs no new RPC.
async fn plain(question: &str, context: &str, ctx: &ToolContext) -> Result<String, RegentError> {
    match ctx.approval.request("ask_user", question, context).await {
        ApprovalDecision::Approve => Ok(json!({"answer": "yes"}).to_string()),
        ApprovalDecision::DenyWithFeedback(text) => Ok(json!({"answer": text}).to_string()),
        ApprovalDecision::Deny => Ok(tool_error_json(
            "no answer (declined or timed out) — proceed on your best judgment and state \
             the assumption you made",
        )),
    }
}

/// Parses and validates the optional `questions` array. `Ok(None)` = the model
/// asked an open question; `Err` = it asked a malformed one.
fn structured(question: &str, args: &Value) -> Result<Option<Questionnaire>, String> {
    let Some(raw) = args.get("questions") else {
        return Ok(None);
    };
    if raw.is_null() || raw.as_array().is_some_and(Vec::is_empty) {
        return Ok(None);
    }
    let questions: Vec<Question> = serde_json::from_value(raw.clone())
        .map_err(|error| format!("ask_user 'questions' is malformed: {error}"))?;
    let questionnaire = Questionnaire {
        // Deterministic from the prompt text so a retried identical call
        // cannot present as a different pending question.
        id: short_id(question),
        questions,
    };
    questionnaire::validate(&questionnaire)?;
    Ok(Some(questionnaire))
}

/// A short, url/callback-safe id derived from the question text. Telegram caps
/// `callback_data` at 64 bytes for the whole composed key, so this stays tiny.
fn short_id(seed: &str) -> String {
    // ponytail: FNV-1a, not a crypto hash — this is a collision-tolerant label,
    // never a security boundary. Swap for a real hash if it ever needs to be one.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in seed.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    format!("q{hash:x}")
}

/// The tool result: one entry per question, keyed by the model's own question
/// id, carrying both the typed answer and a readable label so the model never
/// has to look an option id back up.
fn render_answer(questionnaire: &Questionnaire, answer: &QuestionnaireAnswer) -> String {
    if answer.cancelled && answer.answers.is_empty() {
        return tool_error_json(
            "no answer (dismissed or timed out) — proceed on your best judgment and state \
             the assumption you made",
        );
    }
    let mut out = serde_json::Map::new();
    for question in &questionnaire.questions {
        let given = answer.get(&question.id).unwrap_or(&Answer::Skipped);
        out.insert(
            question.id.clone(),
            json!({
                "answer": questionnaire::describe_answer(question, given),
                "selected": match given {
                    Answer::Selected { option_ids } => json!(option_ids),
                    _ => Value::Null,
                },
                "skipped": matches!(given, Answer::Skipped),
            }),
        );
    }
    json!({"answers": Value::Object(out), "cancelled": answer.cancelled}).to_string()
}

/// Registers `ask_user` (code sessions only — chat has the human in the loop).
pub fn register_ask_user_tool(catalog: &mut ToolCatalog) -> Result<(), RegentError> {
    catalog.register(definition(), Arc::new(AskUserTool))
}

#[cfg(test)]
#[path = "ask_user_tests.rs"]
mod tests;
