//! The settable-env catalog: allowed keys, numbered-key grouping,
//! provider auto-detection, and the `env.list` rows. Split from
//! `env_ops.rs` (file-size rule).

use super::{
    BLOCKED, LLM_KEYS, MANAGED, MAX_KEY_SLOTS, env_var_status, extra_key_groups, key_group,
};
use crate::domain::config::DeaconConfig;
use crate::domain::config::ProviderKind;
use serde_json::{Value, json};

/// If `name` is a numbered key variant (`<BASE>_2`, `<BASE>_3`, …) return its
/// base. Slot 1 is the unsuffixed base, so only `_2` and up count; `_1`/`_0` and
/// non-numeric tails (e.g. `_2X`) are not numbered variants and yield `None`.
pub(super) fn numbered_base(name: &str) -> Option<&str> {
    let (base, suffix) = name.rsplit_once('_')?;
    let n: u32 = suffix.parse().ok()?;
    (2..=MAX_KEY_SLOTS as u32).contains(&n).then_some(base)
}

/// A name is settable if it's a plain UPPER_SNAKE identifier, not blocked, and
/// looks like a credential (a known LLM key or a key/token/secret/url suffix).
/// Numbered variants of a settable base (`OPENROUTER_API_KEY_2`) are settable
/// too — that's the multiple-keys-per-provider convention.
pub(super) fn is_settable(name: &str) -> bool {
    if name.is_empty()
        || BLOCKED.contains(&name)
        || !name
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        || !name.as_bytes()[0].is_ascii_uppercase()
    {
        return false;
    }
    // A numbered variant is settable iff its base is (and the base isn't blocked).
    let base = numbered_base(name).unwrap_or(name);
    if BLOCKED.contains(&base) {
        return false;
    }
    LLM_KEYS.iter().any(|(k, _)| *k == base)
        || [
            "_API_KEY", "_TOKEN", "_SECRET", "_KEY", "_URL", "_CX", "_ID", "_SID",
        ]
        .iter()
        .any(|suf| base.ends_with(suf))
}

/// A just-saved key is invisible in Settings → Model until a `providers:`
/// entry exists (the picker lists only `config.providers`) — so when the var
/// is the conventional key of a known provider kind and config has no provider
/// of that kind (nor any entry reading this var), return the `(dotted path,
/// value)` for the minimal entry to add. `None` = nothing to add: generic/
/// non-provider keys, kinds already configured, or a name already taken
/// (never overwrite a user's entry).
pub(super) fn auto_provider(cfg: &DeaconConfig, saved: &str) -> Option<(String, Value)> {
    let base = numbered_base(saved).unwrap_or(saved);
    // Conventional vars are `<KIND>_API_KEY` with the kind's wire name — the
    // round-trip check below rejects lookalikes (TAVILY_…, REGENT_API_KEY).
    let kind_name = base.strip_suffix("_API_KEY")?.to_ascii_lowercase();
    let kind = ProviderKind::parse(&kind_name)?;
    if kind.key_env_var() != base
        || cfg.providers.contains_key(&kind_name)
        || cfg
            .providers
            .values()
            .any(|s| s.kind == kind || s.api_key_env == base)
    {
        return None;
    }
    Some((
        format!("providers.{kind_name}"),
        json!({ "kind": kind_name, "api_key_env": base }),
    ))
}

/// One `env.list` row: name/label, set-state, masked tail (never the value),
/// and the UI `group` ("llm" | "messaging" | "search" | "speech").
pub(super) fn key_row(name: &str, label: &str, group: &str) -> Value {
    let (set, masked) = env_var_status(name);
    json!({ "name": name, "label": label, "set": set, "masked": masked, "group": group })
}

/// The full managed key set for `env.list`: the LLM provider keys (incl. the
/// generic REGENT_API_KEY fallback), then the messaging/search/speech keys from
/// the shared `MANAGED` table (its LLM entries are already covered by LLM_KEYS).
/// Numbered multi-key slots (`<BASE>_2`…) are listed only when actually set,
/// right after their base row.
pub(super) fn env_key_rows() -> Vec<Value> {
    let mut triples: Vec<(&str, String, &str)> = LLM_KEYS
        .iter()
        .map(|(name, label)| (*name, (*label).to_owned(), "llm"))
        .collect();
    triples.extend(
        MANAGED
            .iter()
            .filter(|(name, _)| key_group(name) != "llm")
            .map(|(name, label)| (*name, (*label).to_owned(), key_group(name))),
    );
    // Keys serving several generation products (Kling/Higgsfield do video AND
    // photo) get one extra row per additional group — same env var either way.
    let extras: Vec<(&str, String, &str)> = triples
        .iter()
        .flat_map(|(name, label, _)| {
            extra_key_groups(name)
                .iter()
                .map(|g| (*name, label.clone(), *g))
                .collect::<Vec<_>>()
        })
        .collect();
    triples.extend(extras);
    let mut rows = Vec::new();
    for (name, label, group) in triples {
        rows.push(key_row(name, &label, group));
        for slot in 2..=MAX_KEY_SLOTS {
            let var = format!("{name}_{slot}");
            if env_var_status(&var).0 {
                rows.push(key_row(&var, &format!("{label} ({slot})"), group));
            }
        }
    }
    rows
}
