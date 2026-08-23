//! The structured-question round trip: a turn that calls `ask_user` with
//! `questions` parks, the client answers over `question.respond`, and the turn
//! resumes with a typed answer. Plus the compatibility arm that matters most —
//! a client that never declared the capability still gets a question it can
//! actually answer, as numbered text down the existing approval path.

use crate::helpers::{ScriptedProvider, make_session_manager};
use regent_deacon::{Dispatcher, RpcRequest};
use regent_kernel::{ChatMessage, ToolCall};
use serde_json::{Value, json};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

/// A provider that asks one structured question, then reports what came back.
fn asks_a_question() -> Arc<ScriptedProvider> {
    let call = or_core::TokenUsage {
        prompt_tokens: 10,
        completion_tokens: 5,
        total_tokens: 15,
        ..Default::default()
    };
    ScriptedProvider::with(vec![
        regent_providers::ChatResponse {
            message: ChatMessage::assistant(
                None,
                vec![ToolCall {
                    id: "c1".into(),
                    name: "ask_user".into(),
                    arguments: json!({
                        "question": "How should I indent?",
                        "questions": [{
                            "id": "style",
                            "prompt": "Tabs or spaces?",
                            "header": "Indent",
                            "kind": "single_select",
                            "options": [
                                {"id": "tab", "label": "Tabs"},
                                {"id": "space", "label": "Spaces"},
                            ],
                        }],
                    })
                    .to_string(),
                }],
            ),
            usage: call,
            finish_reason: Some("tool_calls".into()),
        },
        ScriptedProvider::text_reply("PLAN: use spaces"),
    ])
}

/// Pull notifications until one matches `method`, so unrelated `tool.start` /
/// `turn.*` traffic can't make the assertion flaky.
async fn wait_for(rx: &mut UnboundedReceiver<String>, method: &str) -> Value {
    while let Some(line) = rx.recv().await {
        let value: Value = serde_json::from_str(&line).unwrap();
        if value["method"] == method {
            return value;
        }
    }
    panic!("{method} never arrived");
}

#[tokio::test]
async fn a_capable_client_gets_a_card_and_answers_it_typed() {
    let dir = TempDir::new().unwrap();
    let (sm, mut rx) = make_session_manager(&dir, asks_a_question());
    sm.declare_capabilities(&["questions".to_owned()]);
    assert!(sm.client_supports_questions());

    let manager = Arc::clone(&sm);
    let turn = tokio::spawn(async move { manager.code_plan("indent it", None, None).await });

    let notification = wait_for(&mut rx, "question.request").await;
    let questionnaire = &notification["params"]["questionnaire"];
    assert_eq!(questionnaire["questions"][0]["id"], "style");
    assert_eq!(questionnaire["questions"][0]["header"], "Indent");
    assert_eq!(questionnaire["questions"][0]["kind"], "single_select");
    // `allow_custom` defaults on, so the free-text row is always offered.
    assert_eq!(questionnaire["questions"][0]["allow_custom"], true);

    let session_id = notification["params"]["session_id"].as_str().unwrap();
    let (tx, mut out_rx) = unbounded_channel();
    let dispatcher = Dispatcher::new(Arc::clone(&sm), tx);
    dispatcher
        .handle(RpcRequest {
            jsonrpc: "2.0".into(),
            method: "question.respond".into(),
            params: json!({
                "session_id": session_id,
                "answer": {
                    "questionnaire_id": questionnaire["id"],
                    "answers": [["style", {"kind": "selected", "option_ids": ["space"]}]],
                },
            }),
            id: Some(json!(1)),
        })
        .await;
    let response: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert_eq!(response["result"]["resolved"], true, "{response}");

    let (_, plan) = turn
        .await
        .unwrap()
        .expect("the turn resumes after an answer");
    assert_eq!(plan, "PLAN: use spaces");
}

#[tokio::test]
async fn an_old_client_gets_numbered_text_on_the_existing_channel() {
    let dir = TempDir::new().unwrap();
    let (sm, mut rx) = make_session_manager(&dir, asks_a_question());
    // No declare_capabilities() — this is today's shipped CLI and app.
    assert!(!sm.client_supports_questions());

    let manager = Arc::clone(&sm);
    let turn = tokio::spawn(async move { manager.code_plan("indent it", None, None).await });

    // The card is flattened onto `approval.request`, which every shipped
    // client already renders and can answer.
    let notification = wait_for(&mut rx, "approval.request").await;
    let action = notification["params"]["action"].as_str().unwrap();
    assert!(action.contains("Tabs or spaces?"), "{action}");
    assert!(action.contains("1. Tabs"), "{action}");
    assert!(action.contains("2. Spaces"), "{action}");
    assert!(action.contains("reply with a number"), "{action}");

    // …and the plain-text reply comes back typed to the model.
    let session_id = notification["params"]["session_id"].as_str().unwrap();
    assert!(
        sm.resolve_approval(
            &regent_kernel::SessionId::from_string(session_id),
            false,
            Some("2".to_owned()),
        )
        .await
    );
    let (_, plan) = turn.await.unwrap().expect("the turn resumes after a reply");
    assert_eq!(plan, "PLAN: use spaces");
}

#[tokio::test]
async fn a_stale_answer_is_dropped_rather_than_put_in_the_users_mouth() {
    let dir = TempDir::new().unwrap();
    let (sm, mut rx) = make_session_manager(&dir, asks_a_question());
    sm.declare_capabilities(&["questions".to_owned()]);

    let manager = Arc::clone(&sm);
    let turn = tokio::spawn(async move { manager.code_plan("indent it", None, None).await });
    let notification = wait_for(&mut rx, "question.request").await;
    let session_id = notification["params"]["session_id"].as_str().unwrap();

    // An answer to a DIFFERENT questionnaire — a card left open from an
    // earlier turn. It resolves the wait (the slot was taken) but must not
    // become the user's answer.
    assert!(
        sm.resolve_question(
            &regent_kernel::SessionId::from_string(session_id),
            regent_kernel::QuestionnaireAnswer {
                questionnaire_id: "some-older-card".to_owned(),
                answers: vec![(
                    "style".to_owned(),
                    regent_kernel::Answer::Selected {
                        option_ids: vec!["tab".to_owned()],
                    },
                )],
                cancelled: false,
            },
        )
        .await
    );
    let (session, _) = turn.await.unwrap().expect("the turn still finishes");

    // The model was told nobody answered, not that the user chose "Tabs".
    let store = regent_store::Store::open(&dir.path().join("state.db")).unwrap();
    let history = store.get_conversation(&session).unwrap();
    let tool_result = history
        .iter()
        .find(|m| {
            m.message
                .content
                .as_deref()
                .is_some_and(|c| c.contains("error"))
        })
        .expect("the tool reported no answer");
    let content = tool_result.message.content.as_deref().unwrap();
    assert!(content.contains("best judgment"), "{content}");
    assert!(!content.contains("Tabs"), "{content}");
}

#[tokio::test]
async fn answering_twice_is_a_visible_no_op() {
    let dir = TempDir::new().unwrap();
    let (sm, _rx) = make_session_manager(&dir, ScriptedProvider::with(vec![]));
    let session_id = sm.create_session().await.unwrap();
    // Nothing is pending, so a stray answer reports `resolved: false` rather
    // than being swallowed — a second click is visibly a no-op.
    assert!(
        !sm.resolve_question(
            &session_id,
            regent_kernel::QuestionnaireAnswer {
                questionnaire_id: "q".to_owned(),
                answers: Vec::new(),
                cancelled: false,
            },
        )
        .await
    );
}

/// The bug this guards: `ask_user` used to be registered for code sessions
/// ONLY, on the rule that "chat already has the human in the loop". That was
/// true when the tool could only ask an open question — it stopped being true
/// the day the card shipped. A chat asked for a questionnaire had no tool to
/// call, so it improvised: one model wrote an HTML file and opened it in a
/// browser, another printed a markdown list. Both are the thing the card
/// replaces, and neither returns a typed answer.
#[tokio::test]
async fn a_chat_session_can_actually_ask() {
    use regent_providers::{ChatProvider, ChatRequest, ChatResponse, ProviderError};
    use std::sync::Mutex;

    /// Records the tool names the model was offered — the only way to prove
    /// the catalog reached the provider, rather than that a builder ran.
    struct Recorder {
        offered: Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl ChatProvider for Recorder {
        async fn complete(&self, req: &ChatRequest) -> Result<ChatResponse, ProviderError> {
            *self.offered.lock().unwrap() = req.tools.iter().map(|t| t.name.clone()).collect();
            Ok(ScriptedProvider::text_reply("ok"))
        }
        fn model(&self) -> &str {
            "recorder"
        }
    }

    let dir = TempDir::new().unwrap();
    let provider = Arc::new(Recorder {
        offered: Mutex::new(Vec::new()),
    });
    let as_provider: Arc<dyn ChatProvider> = Arc::clone(&provider) as Arc<dyn ChatProvider>;
    let (sm, _rx) = make_session_manager(&dir, as_provider);
    sm.install_admin(regent_deacon::AdminDeps::default());

    let session = sm.create_session().await.unwrap();
    sm.run_turn(&session, "please ask me 3 questions")
        .await
        .unwrap();

    let offered = provider.offered.lock().unwrap().clone();
    assert!(
        offered.iter().any(|n| n == "ask_user"),
        "a chat session must be able to ask a structured question; offered: {offered:?}"
    );
    // What the tool costs a chat turn, in the same estimator the catalog gate
    // uses. Printed rather than asserted: the gate measures `fixed_prefix`, so
    // it would not have caught this growth, and a number in the log beats a
    // silent regression. See the ceiling note in tiering.rs.
    let schema = serde_json::to_string(&regent_tools::ask_user_definition()).unwrap();
    println!(
        "ask_user schema ~{} tokens",
        schema.chars().count().div_ceil(4)
    );
}

/// The escalation arm of the same bug, and the one that bites a real
/// conversation rather than a first turn. `escalate_to_full` rebuilds the
/// catalog from `make_catalogs_and_prompt` and used to stop there, while
/// `create_session_keyed` registers `ask_user` AFTER that call — so a plain
/// chat that reached for an agentic tool silently lost the ability to ask for
/// the rest of its life. It could ask on turn one and not on turn three, which
/// is far harder to notice than never being able to ask at all.
#[tokio::test]
async fn a_chat_session_can_still_ask_after_it_escalates() {
    use regent_providers::{ChatProvider, ChatRequest, ChatResponse, ProviderError};
    use std::sync::Mutex;

    /// Reaches for an agentic tool on the first call (the escalation trigger),
    /// then records what the escalated catalog actually offered.
    struct Escalator {
        offered: Mutex<Vec<String>>,
        calls: Mutex<usize>,
    }

    #[async_trait::async_trait]
    impl ChatProvider for Escalator {
        async fn complete(&self, req: &ChatRequest) -> Result<ChatResponse, ProviderError> {
            *self.offered.lock().unwrap() = req.tools.iter().map(|t| t.name.clone()).collect();
            let mut calls = self.calls.lock().unwrap();
            *calls += 1;
            Ok(if *calls == 1 {
                ChatResponse {
                    message: ChatMessage::assistant(
                        None,
                        vec![ToolCall {
                            id: "call_1".into(),
                            name: "load_tools".into(),
                            arguments: r#"{"names":["kanban"]}"#.into(),
                        }],
                    ),
                    usage: or_core::TokenUsage::default(),
                    finish_reason: Some("tool_calls".into()),
                }
            } else {
                ScriptedProvider::text_reply("ok")
            })
        }
        fn model(&self) -> &str {
            "escalator"
        }
    }

    let dir = TempDir::new().unwrap();
    let provider = Arc::new(Escalator {
        offered: Mutex::new(Vec::new()),
        calls: Mutex::new(0),
    });
    let as_provider: Arc<dyn ChatProvider> = Arc::clone(&provider) as Arc<dyn ChatProvider>;
    let (sm, _rx) = make_session_manager(&dir, as_provider);
    sm.install_admin(regent_deacon::AdminDeps::default());

    let session = sm.create_session().await.unwrap();
    // Turn 1 reaches for the agentic tool; escalation applies before turn 2.
    sm.run_turn(&session, "load kanban").await.unwrap();
    sm.run_turn(&session, "now ask me 3 questions")
        .await
        .unwrap();

    let offered = provider.offered.lock().unwrap().clone();
    assert!(
        offered.iter().any(|n| n == "ask_user"),
        "an escalated chat session must still be able to ask; offered: {offered:?}"
    );
}
