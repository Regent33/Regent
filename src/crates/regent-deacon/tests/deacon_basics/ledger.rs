//! Stable-Prefix Ledger regression gates (SPL P1, proposal §3.3): the prefix
//! size ceiling, Tier 0/1 hash stability across a synthetic 50-turn session,
//! and the trip tests proving an injected timestamp / unstable tool-schema
//! serialization is DETECTED. A new feature that injects per-turn content into
//! the stable prefix fails here until it budgets it or moves it to Tier 3.

use crate::helpers::{ScriptedProvider, make_session_manager};
use regent_deacon::{Bust, Dispatcher, Ledger, Segment, Tier};
use serde_json::{Value, json};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::mpsc::unbounded_channel;

/// Proposal §3.3 gate (a): the fixed prefix — rendered system prompt plus the
/// serialized default tool catalog — stays under the 80k-char ceiling.
#[tokio::test]
async fn fixed_prefix_stays_under_char_ceiling() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);

    let (prompt, defs) = sm.fixed_prefix().await.unwrap();
    let total = prompt.chars().count() + defs.chars().count();
    assert!(!prompt.is_empty() && !defs.is_empty());
    assert!(
        total < 80_000,
        "fixed prefix grew to {total} chars (ceiling 80k) — budget the addition or defer it"
    );
}

/// Proposal §3.3 gate (b): a 50-turn synthetic session — 50 per-turn hash
/// recomputations against the same entry — never changes the Tier 0/1 hashes.
/// Also covers the §3.1 now_line tier assertion: two builds in the same
/// process (same env, read once at spawn) render identical Tier 0 bytes.
#[tokio::test]
async fn tier_hashes_stable_across_fifty_synthetic_turns() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let sid = sm.create_session().await.unwrap();

    let baseline = sm.turn_prefix_hashes(&sid).await.unwrap();
    assert_eq!(baseline.0.len(), 16, "tier0 hash is 16 hex chars");
    assert_eq!(baseline.1.len(), 16, "tier1 hash is 16 hex chars");
    for turn in 1..=50 {
        let now = sm.turn_prefix_hashes(&sid).await.unwrap();
        assert_eq!(
            now, baseline,
            "tier hashes drifted at synthetic turn {turn}"
        );
    }

    // Same process, second session build: identical Tier 0 AND Tier 1 bytes
    // (env lines are read-once; persona/skills/memory stores are unchanged).
    let sid2 = sm.create_session().await.unwrap();
    let second = sm.turn_prefix_hashes(&sid2).await.unwrap();
    assert_eq!(second, baseline, "two same-process builds must hash equal");
}

/// The pure check-loop variant of gate (b): re-serializing the definitions
/// every iteration is what catches serialization instability (a HashMap
/// sneaking into a schema). 50 re-serializations, zero busts.
#[tokio::test]
async fn reserializing_definitions_fifty_times_never_busts() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);

    let (prompt, defs) = sm.fixed_prefix().await.unwrap();
    let mut ledger = Ledger::new(vec![Segment::tier0("prompt", prompt.clone())]);
    ledger.seal(&defs);
    for turn in 1..=50 {
        let (_, fresh_defs) = sm.fixed_prefix().await.unwrap();
        let busts = ledger.check(&prompt, &fresh_defs);
        assert!(
            busts.is_empty(),
            "definitions re-serialization drifted at iteration {turn}: {busts:?}"
        );
    }
}

/// The tier hashes ride the SUCCESS `turn.complete` as additive fields and
/// stay identical across turns of one session — what desktop/CLI clients
/// watch to spot a busted prefix.
#[tokio::test]
async fn turn_complete_carries_stable_tier_hashes() {
    let dir = TempDir::new().unwrap();
    // Generous script: the post-turn background review fork shares this
    // provider and may consume replies between the two real turns.
    let provider = ScriptedProvider::with(vec![ScriptedProvider::text_reply("ok"); 10]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let d = Dispatcher::new(Arc::clone(&sm), tx);
    let sid = sm.create_session().await.unwrap();
    // Pre-title so first-turn title generation (a detached model call) can't
    // steal a scripted reply or interleave extra notifications.
    sm.rename_session(&sid, "spl test").unwrap();

    let mut hashes = Vec::new();
    for turn in 0..2 {
        d.handle(regent_deacon::RpcRequest {
            jsonrpc: "2.0".into(),
            method: "prompt.submit".into(),
            params: json!({"session_id": sid.to_string(), "text": "go"}),
            id: Some(json!(turn)),
        })
        .await;
        // Read until this turn's `turn.complete`, skipping other traffic.
        loop {
            let line = tokio::time::timeout(std::time::Duration::from_secs(5), out_rx.recv())
                .await
                .expect("stream stalled")
                .expect("channel closed");
            let v: Value = serde_json::from_str(&line).unwrap();
            if v.get("method").and_then(|m| m.as_str()) == Some("turn.complete") {
                let p = &v["params"];
                let t0 = p["tier0_hash"]
                    .as_str()
                    .unwrap_or_else(|| panic!("tier0_hash missing: {p}"))
                    .to_owned();
                let t1 = p["tier1_hash"].as_str().expect("tier1_hash").to_owned();
                assert_eq!(t0.len(), 16, "hex u64: {t0}");
                hashes.push((t0, t1));
                break;
            }
        }
    }
    assert_eq!(hashes.len(), 2);
    assert_eq!(
        hashes[0], hashes[1],
        "tier hashes must not drift across turns"
    );
}

fn sample_segments(now: &str, persona: &str) -> Vec<Segment> {
    vec![
        Segment::tier0("system_prompt", "You are Regent."),
        Segment::tier0(
            "now_line",
            format!("\n\nThe current date and time is {now}."),
        ),
        Segment::tier1("persona", format!("\n\n{persona}")),
        Segment::tier0("capabilities", "\n\nCAPABILITIES…"),
    ]
}

const DEFS: &str = r#"[{"name":"read_file","description":"…"}]"#;

/// Proposal §3.3 gate (c): an injected per-turn timestamp in a Tier 0 segment
/// trips the check, naming the busted tier — the size ceiling alone would
/// never catch this.
#[test]
fn injected_timestamp_trips_the_tier0_check() {
    let mut baseline = Ledger::new(sample_segments("2026-07-10 09:00", "soul"));
    baseline.seal(DEFS);
    // Sanity: the unchanged prompt + defs pass clean.
    assert!(baseline.check(&baseline.render(), DEFS).is_empty());

    // Someone "fixes" now_line to live wall-clock: next turn renders a new
    // timestamp. Detection must name Tier 0 and the segment.
    let drifted = Ledger::new(sample_segments("2026-07-10 09:01", "soul")).render();
    assert_eq!(
        baseline.check(&drifted, DEFS),
        vec![Bust {
            tier: Tier::Process,
            segment: "now_line"
        }]
    );
}

/// Gate (c) continued: mutating the frozen prompt's Tier 1 span and unstable
/// tool-definitions serialization are each detected and attributed; appended
/// content past the stable prefix is flagged too.
#[test]
fn prompt_mutation_defs_drift_and_trailing_injection_are_detected() {
    let mut baseline = Ledger::new(sample_segments("2026-07-10 09:00", "soul"));
    baseline.seal(DEFS);

    // A mid-session write to the Tier 1 persona span of the frozen string.
    let persona_edit = Ledger::new(sample_segments("2026-07-10 09:00", "SOUL v2")).render();
    assert_eq!(
        baseline.check(&persona_edit, DEFS),
        vec![Bust {
            tier: Tier::Session,
            segment: "persona"
        }]
    );

    // Tool-schema serialization instability (e.g. HashMap field ordering).
    let reordered_defs = r#"[{"description":"…","name":"read_file"}]"#;
    assert_eq!(
        baseline.check(&baseline.render(), reordered_defs),
        vec![Bust {
            tier: Tier::Process,
            segment: "tool_definitions"
        }]
    );

    // A per-turn injection appended after every intact segment.
    let appended = format!("{}\n\n[turn 7 status]", baseline.render());
    assert_eq!(
        baseline.check(&appended, DEFS),
        vec![Bust {
            tier: Tier::Session,
            segment: "trailing_injection"
        }]
    );
}

/// The ledger's render is byte-identical to the historical `format!` assembly
/// — the contract that keeps P0's measurements and stored session prompts
/// valid. Separators belong to the segment they precede.
#[test]
fn render_matches_the_historical_format_concatenation() {
    let (system, now, artifacts, persona, caps, skills, memory, voice) = (
        "SYS",
        "\n\nnow",
        "\n\nartifacts",
        "persona",
        "CAPS",
        "skills",
        "memory",
        "\n\nvoice",
    );
    let ledger = Ledger::new(vec![
        Segment::tier0("system_prompt", system),
        Segment::tier0("now_line", now),
        Segment::tier0("artifacts_line", artifacts),
        Segment::tier1("persona", persona),
        Segment::tier0("capabilities", format!("\n\n{caps}")),
        Segment::tier1("skills_index", format!("\n\n{skills}")),
        Segment::tier1("memory", format!("\n\n{memory}")),
        Segment::tier0("voice_line", voice),
    ]);
    let historical =
        format!("{system}{now}{artifacts}{persona}\n\n{caps}\n\n{skills}\n\n{memory}{voice}");
    assert_eq!(ledger.render(), historical);
}
