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
