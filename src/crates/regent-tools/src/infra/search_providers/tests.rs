use super::*;

#[test]
fn names_and_aliases_map_to_providers() {
    for name in [
        "brave",
        "tavily",
        "serpapi",
        "serp",
        "exa",
        "google_cse",
        "cse",
        "ddg",
    ] {
        assert!(provider_from_name(name).is_some(), "{name} should resolve");
    }
    assert!(provider_from_name("nope").is_none());
    // Every auto-select candidate must be a real provider name.
    for (name, _) in KEYED {
        assert!(
            provider_from_name(name).is_some(),
            "KEYED name {name} must resolve"
        );
    }
}
