//! Unit tests for `react_to_message` (a sibling file pulled into the module
//! tree via #[path] — `use super::*` still sees the parent).

use super::*;
use std::sync::Mutex;

/// Records what reached the platform, so a test can assert nothing was sent.
#[derive(Default)]
struct Spy {
    calls: Mutex<Vec<(Option<String>, String)>>,
    fail: bool,
}

#[async_trait]
impl ReactionSink for Spy {
    async fn react(&self, message_id: Option<&str>, emoji: &str) -> Result<(), RegentError> {
        self.calls
            .lock()
            .unwrap()
            .push((message_id.map(str::to_owned), emoji.to_owned()));
        if self.fail {
            return Err(RegentError::Tool {
                tool: "react_to_message".into(),
                message: "telegram says no".into(),
            });
        }
        Ok(())
    }

    fn targets(&self) -> Vec<String> {
        vec!["telegram:42".to_owned()]
    }
}

async fn react(spy: Arc<Spy>, args: Value) -> String {
    let mut catalog = ToolCatalog::new();
    register_reaction_tool(&mut catalog, Arc::clone(&spy) as Arc<dyn ReactionSink>).unwrap();
    let ctx = ToolContext::new(
        std::env::temp_dir(),
        Arc::new(crate::domain::contracts::DenyAll),
    );
    catalog
        .dispatch("react_to_message", &args.to_string(), &ctx)
        .await
}

#[tokio::test]
async fn reacts_to_the_current_message_by_default() {
    let spy = Arc::new(Spy::default());
    let out = react(Arc::clone(&spy), json!({"emoji": "👍"})).await;
    assert!(out.contains("👍"), "{out}");
    assert!(!out.contains("error"), "{out}");
    assert_eq!(*spy.calls.lock().unwrap(), vec![(None, "👍".to_owned())]);
}

#[tokio::test]
async fn reacts_to_a_named_message_when_given_one() {
    let spy = Arc::new(Spy::default());
    react(Arc::clone(&spy), json!({"emoji": "🎉", "message_id": "77"})).await;
    assert_eq!(
        *spy.calls.lock().unwrap(),
        vec![(Some("77".to_owned()), "🎉".to_owned())]
    );
}

#[tokio::test]
async fn a_platform_failure_is_reported_not_swallowed() {
    let spy = Arc::new(Spy {
        fail: true,
        ..Spy::default()
    });
    let out = react(spy, json!({"emoji": "👍"})).await;
    assert!(out.contains("error"), "{out}");
    assert!(out.contains("telegram says no"), "{out}");
}

#[tokio::test]
async fn junk_never_reaches_the_platform() {
    // The emoji is built into a URL path on Discord and a validated enum on
    // Telegram, so rejection has to happen BEFORE any request exists.
    for bad in [
        json!({"emoji": ""}),
        json!({"emoji": "thumbsup"}),
        json!({"emoji": "👍\n"}),
        json!({"emoji": "../../etc/passwd"}),
        json!({"emoji": "👍/@me/../../"}),
        json!({"emoji": "👍👍👍👍👍👍👍👍👍"}),
        json!({"emoji": "👍 "}),
        json!({}),
    ] {
        let spy = Arc::new(Spy::default());
        let out = react(Arc::clone(&spy), bad.clone()).await;
        assert!(out.contains("error"), "should reject {bad}: {out}");
        assert!(
            spy.calls.lock().unwrap().is_empty(),
            "{bad} must never reach the platform"
        );
    }
}

#[test]
fn multi_codepoint_emoji_are_accepted() {
    // A ZWJ sequence, a skin tone, and a variation selector are all one
    // reaction to a human and must not be rejected as "not a single emoji".
    for good in ["👍", "👨‍💻", "👍🏽", "❤️", "🤷‍♀️", "🇵🇭"] {
        assert!(validate_emoji(good).is_ok(), "should accept {good}");
    }
}

#[test]
fn the_schema_names_where_reactions_land() {
    let wired = definition(&["telegram:42".to_owned()]);
    assert!(wired.description.contains("telegram:42"));
    // …and stays sane with no targets wired.
    assert!(!definition(&[]).description.contains("Reacts in"));
}
