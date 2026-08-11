//! Unit tests for `persona` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

#[test]
fn constitution_is_a_valid_seeded_persona_key() {
    assert!(is_valid_persona_key("constitution"));
    let store = Store::open_in_memory().unwrap();
    // Seeded empty on open — opt-in, so it must not render by default.
    assert_eq!(store.get_persona("constitution").unwrap(), "");
    assert!(!store.persona_block().contains("Your constitution"));
}

#[test]
fn persona_writes_are_budgeted_per_key() {
    let store = Store::open_in_memory().unwrap();
    // Within budget → fine.
    store.set_persona("soul", "Call me Reggie.").unwrap();
    // Over budget → the guidance error, nothing written.
    let big = "x".repeat(persona_budget("soul") + 1);
    let err = store.set_persona("soul", &big).unwrap_err();
    assert!(matches!(err, StoreError::PersonaBudget { .. }), "{err}");
    assert_eq!(store.get_persona("soul").unwrap(), "Call me Reggie.");
    // The opt-in constitution gets the most headroom.
    assert!(persona_budget("constitution") > persona_budget("soul"));
    assert!(persona_budget("about.identity") < persona_budget("about"));
}

/// The gate must measure the same unit the deacon's Tier-1 ceiling measures.
///
/// While this counted CHARS and the ceiling counted BYTES, a persona in a
/// 3-byte script written to exactly the advertised limit was accepted here and
/// arrived at the ceiling three times oversized — which then deleted the memory
/// block and the skills index to make room, silently. Only users writing
/// Japanese, Chinese, Korean, Cyrillic, Greek, Arabic or Hebrew could reach it,
/// which is why it survived every English-language test in this file.
#[test]
fn the_budget_counts_bytes_so_a_multibyte_persona_cannot_smuggle_past_the_ceiling() {
    let store = Store::open_in_memory().unwrap();
    let limit = persona_budget("soul");
    // Exactly the CHARACTER limit, in a 3-byte script: the old gate waved this
    // through at 3x the ceiling's unit.
    let cjk = "経".repeat(limit);
    assert!(cjk.chars().count() == limit && cjk.len() == limit * 3);
    let err = store.set_persona("soul", &cjk).unwrap_err();
    assert!(matches!(err, StoreError::PersonaBudget { .. }), "{err}");
    // Rejected loudly and nothing written — recoverable, unlike a memory block
    // that quietly vanishes on the next session build. Compared against
    // DEFAULT_SOUL, not "": a fresh store SEEDS soul (see `seed_persona`), so
    // an empty-string assertion here would fail for a reason that has nothing
    // to do with the budget.
    assert_eq!(store.get_persona("soul").unwrap(), DEFAULT_SOUL);
    // The same script still fits when it genuinely fits in bytes.
    let ok = "経".repeat(limit / 3);
    store.set_persona("soul", &ok).unwrap();
    assert_eq!(store.get_persona("soul").unwrap().len(), (limit / 3) * 3);
}

#[test]
fn constitution_renders_first_in_the_persona_block() {
    let store = Store::open_in_memory().unwrap();
    store
        .set_persona("constitution", "Love is patient.")
        .unwrap();
    store.set_persona("soul", "Call me Reggie.").unwrap();
    let block = store.persona_block();
    let c = block
        .find("Your constitution")
        .expect("constitution header");
    let s = block.find("Your persona").expect("soul header");
    assert!(c < s, "constitution must precede the soul");
    assert!(block.contains("Love is patient."));
}

// An install predating the soul feature carries an EXISTING empty soul row —
// INSERT OR IGNORE skips it forever (the owner's own install woke up
// personality-less this way). Re-seeding must backfill it.
#[test]
fn seed_backfills_a_preexisting_empty_soul_row() {
    let store = Store::open_in_memory().unwrap();
    store.set_persona("soul", "").unwrap();
    store.seed_persona().unwrap();
    assert_eq!(
        store.get_persona("soul").unwrap(),
        DEFAULT_SOUL,
        "an empty soul row is never a deliberate state — backfilled on seed"
    );
    // A user-EDITED soul is untouched by re-seeding.
    store.set_persona("soul", "custom voice").unwrap();
    store.seed_persona().unwrap();
    assert_eq!(store.get_persona("soul").unwrap(), "custom voice");
}
