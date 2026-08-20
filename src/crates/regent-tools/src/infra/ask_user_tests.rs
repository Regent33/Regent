//! Unit tests for `ask_user` (a sibling file pulled into the module tree via
//! #[path] — `use super::*` still sees the parent).

use super::*;
use crate::domain::contracts::ApprovalHandler;

struct Scripted(ApprovalDecision);

#[async_trait]
impl ApprovalHandler for Scripted {
    async fn request(&self, _: &str, _: &str, _: &str) -> ApprovalDecision {
        self.0.clone()
    }
}

/// A surface that draws a real card: it never sees text, it answers typed.
struct Card(QuestionnaireAnswer);

#[async_trait]
impl ApprovalHandler for Card {
    async fn request(&self, _: &str, _: &str, _: &str) -> ApprovalDecision {
        unreachable!("a card surface never falls back to text")
    }
    async fn request_structured(&self, _: &Questionnaire) -> QuestionnaireAnswer {
        self.0.clone()
    }
}

async fn dispatch(handler: Arc<dyn ApprovalHandler>, args: Value) -> String {
    let mut catalog = ToolCatalog::new();
    register_ask_user_tool(&mut catalog).unwrap();
    let ctx = ToolContext::new(std::env::temp_dir(), handler);
    catalog.dispatch("ask_user", &args.to_string(), &ctx).await
}

async fn ask(decision: ApprovalDecision) -> String {
    dispatch(
        Arc::new(Scripted(decision)),
        json!({"question": "tabs or spaces?"}),
    )
    .await
}

fn sheet() -> Value {
    json!([{
        "id": "style",
        "prompt": "Tabs or spaces?",
        "header": "Indent",
        "kind": "single_select",
        "options": [
            {"id": "tab", "label": "Tabs"},
            {"id": "space", "label": "Spaces", "description": "two-wide"},
        ],
    }])
}

#[tokio::test]
async fn maps_each_decision_to_an_answer() {
    assert_eq!(ask(ApprovalDecision::Approve).await, r#"{"answer":"yes"}"#);
    assert_eq!(
        ask(ApprovalDecision::DenyWithFeedback("spaces, 2".into())).await,
        r#"{"answer":"spaces, 2"}"#
    );
    assert!(ask(ApprovalDecision::Deny).await.contains("error"));
}

#[tokio::test]
async fn a_card_surface_answers_typed() {
    let answer = QuestionnaireAnswer {
        questionnaire_id: "ignored".into(),
        answers: vec![(
            "style".into(),
            Answer::Selected {
                option_ids: vec!["space".into()],
            },
        )],
        cancelled: false,
    };
    let out = dispatch(
        Arc::new(Card(answer)),
        json!({"question": "how should I indent?", "questions": sheet()}),
    )
    .await;
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["answers"]["style"]["answer"], "Spaces");
    assert_eq!(parsed["answers"]["style"]["selected"][0], "space");
    assert_eq!(parsed["answers"]["style"]["skipped"], false);
    assert_eq!(parsed["cancelled"], false);
}

#[tokio::test]
async fn a_text_only_surface_still_answers_the_card() {
    // The trait's default renders numbered text and parses "2" back to the
    // second option — no per-surface code, and the model still gets a label.
    let out = dispatch(
        Arc::new(Scripted(ApprovalDecision::DenyWithFeedback("2".into()))),
        json!({"question": "how should I indent?", "questions": sheet()}),
    )
    .await;
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["answers"]["style"]["answer"], "Spaces");
    assert_eq!(parsed["answers"]["style"]["selected"][0], "space");
}

#[tokio::test]
async fn a_dismissed_card_reads_as_no_answer() {
    let out = dispatch(
        Arc::new(Scripted(ApprovalDecision::Deny)),
        json!({"question": "how should I indent?", "questions": sheet()}),
    )
    .await;
    assert!(out.contains("error"), "{out}");
    assert!(out.contains("best judgment"), "{out}");
}

#[tokio::test]
async fn a_malformed_card_is_returned_to_the_model_not_the_human() {
    let bad = [
        // A select with one option is a statement, not a question.
        json!([{"id": "a", "prompt": "?", "kind": "single_select",
                "options": [{"id": "x", "label": "X"}]}]),
        // Options on a confirm.
        json!([{"id": "a", "prompt": "?", "kind": "confirm",
                "options": [{"id": "x", "label": "X"}, {"id": "y", "label": "Y"}]}]),
        // An unknown kind.
        json!([{"id": "a", "prompt": "?", "kind": "slider"}]),
        // Duplicate question ids.
        json!([{"id": "a", "prompt": "?", "kind": "text"},
               {"id": "a", "prompt": "?", "kind": "text"}]),
    ];
    for questions in bad {
        let out = dispatch(
            // Any reach for the human would panic this surface.
            Arc::new(Card(QuestionnaireAnswer {
                questionnaire_id: String::new(),
                answers: Vec::new(),
                cancelled: false,
            })),
            json!({"question": "q", "questions": questions}),
        )
        .await;
        assert!(out.contains("error"), "should reject: {questions} -> {out}");
    }
}

#[tokio::test]
async fn an_empty_questions_array_is_the_plain_path() {
    // A model that sends `questions: []` asked an open question — it must not
    // trip validation and lose the turn.
    let out = dispatch(
        Arc::new(Scripted(ApprovalDecision::DenyWithFeedback(
            "spaces".into(),
        ))),
        json!({"question": "how should I indent?", "questions": []}),
    )
    .await;
    assert_eq!(out, r#"{"answer":"spaces"}"#);
}

#[test]
fn ids_are_short_stable_and_callback_safe() {
    let id = short_id("how should I indent this file?");
    assert_eq!(id, short_id("how should I indent this file?"));
    assert_ne!(id, short_id("something else"));
    // Telegram caps composed callback_data at 64 bytes for id:question:option.
    assert!(id.len() <= 17, "{id} is too long for a callback key");
    assert!(id.chars().all(|c| c.is_ascii_alphanumeric()), "{id}");
}
