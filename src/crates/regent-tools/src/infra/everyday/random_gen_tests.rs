use super::*;
use crate::domain::contracts::DenyAll;
use std::sync::Arc;

fn ctx() -> ToolContext {
    ToolContext::new(std::path::PathBuf::from("."), Arc::new(DenyAll))
}

#[tokio::test]
async fn dice_rolls_land_within_the_declared_range() {
    let ctx = ctx();
    let out = RandomGenTool
        .execute(json!({"mode": "dice", "notation": "3d6"}), &ctx)
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    let rolls = v["rolls"].as_array().unwrap();
    assert_eq!(rolls.len(), 3, "{v}");
    for roll in rolls {
        let n = roll.as_u64().unwrap();
        assert!((1..=6).contains(&n), "{v}");
    }
    let total: u64 = rolls.iter().map(|r| r.as_u64().unwrap()).sum();
    assert_eq!(v["total"].as_u64().unwrap(), total, "{v}");
}

#[tokio::test]
async fn dice_notation_over_the_cap_is_rejected() {
    let ctx = ctx();
    let out = RandomGenTool
        .execute(json!({"mode": "dice", "notation": "2000d6"}), &ctx)
        .await
        .unwrap();
    assert!(out.contains("exceeds the limit"), "got: {out}");
}

#[tokio::test]
async fn coin_flips_are_heads_or_tails_and_counted() {
    let ctx = ctx();
    let out = RandomGenTool
        .execute(json!({"mode": "coin", "count": 20}), &ctx)
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    let flips = v["flips"].as_array().unwrap();
    assert_eq!(flips.len(), 20, "{v}");
    let heads = v["heads"].as_u64().unwrap();
    let tails = v["tails"].as_u64().unwrap();
    assert_eq!(heads + tails, 20, "{v}");
}

#[tokio::test]
async fn pick_returns_distinct_items_from_the_list() {
    let ctx = ctx();
    let out = RandomGenTool
        .execute(
            json!({"mode": "pick", "items": ["a", "b", "c", "d"], "count": 2}),
            &ctx,
        )
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    let picked = v["picked"].as_array().unwrap();
    assert_eq!(picked.len(), 2, "{v}");
    assert_ne!(picked[0], picked[1], "{v}");
}

#[tokio::test]
async fn pick_more_than_available_is_an_error() {
    let ctx = ctx();
    let out = RandomGenTool
        .execute(
            json!({"mode": "pick", "items": ["a", "b"], "count": 5}),
            &ctx,
        )
        .await
        .unwrap();
    assert!(out.contains("between 1 and the list length"), "got: {out}");
}

#[tokio::test]
async fn shuffle_preserves_every_element() {
    let ctx = ctx();
    let out = RandomGenTool
        .execute(json!({"mode": "shuffle", "items": [1, 2, 3, 4, 5]}), &ctx)
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    let mut shuffled: Vec<u64> = v["shuffled"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_u64().unwrap())
        .collect();
    shuffled.sort_unstable();
    assert_eq!(shuffled, vec![1, 2, 3, 4, 5], "{v}");
}

#[tokio::test]
async fn password_respects_requested_length_and_charset() {
    let ctx = ctx();
    let out = RandomGenTool
        .execute(
            json!({"mode": "password", "length": 24, "uppercase": true, "lowercase": false, "digits": true, "symbols": false}),
            &ctx,
        )
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    let password = v["password"].as_str().unwrap();
    assert_eq!(password.len(), 24, "{v}");
    assert!(
        password
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()),
        "{v}"
    );
}

#[tokio::test]
async fn password_with_no_charset_enabled_is_an_error() {
    let ctx = ctx();
    let out = RandomGenTool
        .execute(
            json!({"mode": "password", "uppercase": false, "lowercase": false, "digits": false, "symbols": false}),
            &ctx,
        )
        .await
        .unwrap();
    assert!(out.contains("must be enabled"), "got: {out}");
}

#[tokio::test]
async fn missing_mode_is_an_error() {
    let ctx = ctx();
    let out = RandomGenTool.execute(json!({}), &ctx).await.unwrap();
    assert!(out.contains("missing required parameter"), "got: {out}");
}
