//! Unit tests for `profiles` (extracted for the file-size rule).

use crate::infra::db::Store;

fn store() -> Store {
    Store::open_in_memory().unwrap()
}

#[test]
fn default_profile_always_exists_and_is_active() {
    let store = store();
    assert_eq!(store.active_profile(), "default");
    assert_eq!(store.list_profiles().unwrap(), vec!["default"]);
}

#[test]
fn create_switch_and_namespaced_persona_roundtrip() {
    let store = store();
    store.set_persona("soul", "default soul").unwrap();
    store.create_profile("work").unwrap();
    store.switch_profile("work").unwrap();
    assert_eq!(store.active_profile(), "work");
    // The new profile starts empty; writes land in its namespace.
    assert_eq!(store.get_persona("soul").unwrap(), "");
    store.set_persona("soul", "work soul").unwrap();
    assert_eq!(store.get_persona("soul").unwrap(), "work soul");
    // Switching back restores the untouched default persona.
    store.switch_profile("default").unwrap();
    assert_eq!(store.get_persona("soul").unwrap(), "default soul");
    assert_eq!(store.list_profiles().unwrap(), vec!["default", "work"]);
}

#[test]
fn constitution_is_global_across_profiles() {
    let store = store();
    store
        .set_persona("constitution", "the values layer")
        .unwrap();
    store.create_profile("alt").unwrap();
    store.switch_profile("alt").unwrap();
    assert_eq!(
        store.get_persona("constitution").unwrap(),
        "the values layer",
        "ADR-028: a profile switch must never swap the constitution"
    );
}

#[test]
fn bad_names_duplicates_and_unknown_switch_are_rejected() {
    let store = store();
    assert!(store.create_profile("Bad Name!").is_err());
    assert!(store.create_profile("").is_err());
    assert!(store.create_profile("default").is_err(), "default is taken");
    store.create_profile("work").unwrap();
    assert!(store.create_profile("work").is_err(), "duplicate");
    assert!(store.switch_profile("nope").is_err(), "unknown profile");
}
