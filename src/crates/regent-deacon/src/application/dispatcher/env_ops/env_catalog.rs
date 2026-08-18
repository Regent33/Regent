//! The settable-env catalog: allowed keys, numbered-key grouping,
//! provider auto-detection, and the `env.list` rows. Split from
//! `env_ops.rs` (file-size rule).

use super::{
    BLOCKED, MANAGED, MAX_KEY_SLOTS, env_var_status, extra_key_groups, key_group, llm_keys,
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
    (suffix == n.to_string() && (2..=MAX_KEY_SLOTS as u32).contains(&n)).then_some(base)
}

/// A name is settable only when its base is in the catalog rendered by
/// `env.list`. Numbered variants (`OPENROUTER_API_KEY_2`) are allowed only for
/// canonical slots 2 through [`MAX_KEY_SLOTS`]. Exact membership matters here:
/// credential-looking names such as `DATABASE_URL` must not turn the API Keys
/// RPC into an arbitrary process-environment write primitive.
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
    llm_keys().iter().any(|(k, _)| *k == base)
        || MANAGED.iter().any(|(key, _)| *key == base)
        || regent_speech::SPEECH_PROVIDERS
            .iter()
            .any(|provider| provider.key_var == Some(base))
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

/// One row per speech backend that takes a key, **derived from
/// [`regent_speech::SPEECH_PROVIDERS`]** — the same table `voice.models` builds
/// the picker from, so a backend can never be offered in the voice menu with no
/// way to authenticate it (ADR-045). Hand-listing these is how AI/ML API, Azure
/// and RunPod ended up settable-but-invisible.
///
/// A key shared with an LLM provider (Groq, OpenAI, DashScope) gets a row here
/// *as well as* its llm row: one secret, listed in both places it's looked for.
fn speech_key_rows() -> Vec<(&'static str, String)> {
    let mut rows: Vec<(&'static str, String)> = Vec::new();
    for provider in regent_speech::SPEECH_PROVIDERS {
        let Some(var) = provider.key_var else {
            continue;
        };
        if rows.iter().any(|(name, _)| *name == var) {
            continue;
        }
        rows.push((var, format!("{} speech key", provider.label)));
    }
    rows
}

/// One `env.list` row: name/label, set-state, masked tail (never the value),
/// and the UI `group` (for example "llm", "local", "speech", or "vision").
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
    let mut triples: Vec<(&str, String, &str)> = llm_keys()
        .into_iter()
        .map(|(name, label)| (name, label.to_owned(), key_group(name)))
        .collect();
    for (name, label) in MANAGED {
        let group = key_group(name);
        if group != "llm"
            && !triples
                .iter()
                .any(|(existing, _, existing_group)| existing == name && *existing_group == group)
        {
            triples.push((*name, (*label).to_owned(), group));
        }
    }
    // Per-provider speech keys. A var MANAGED already words for itself in this
    // group (`REGENT_SPEECH_API_KEY`, `LEMONFOX_API_KEY`) keeps its wording.
    let speech: Vec<(&str, String, &str)> = speech_key_rows()
        .into_iter()
        .filter(|(name, _)| {
            !triples
                .iter()
                .any(|(existing, _, group)| existing == name && *group == "speech")
        })
        .map(|(name, label)| (name, label, "speech"))
        .collect();
    triples.extend(speech);
    // A key may serve several real product groups; extra_key_groups keeps that
    // additive without duplicating env storage.
    //
    // Deduplicated on (name, group), because a key can ALREADY appear in
    // `triples` more than once: OPENAI_API_KEY is both an LLM row and a speech
    // row, so a plain flat_map emitted its image and video rows twice each and
    // the page drew "OpenAI" twice under Image generation. Also skips an extra
    // group that is simply the key's primary one.
    let mut extras: Vec<(&str, String, &str)> = Vec::new();
    for (name, label, _) in &triples {
        for group in extra_key_groups(name) {
            let listed = |rows: &[(&str, String, &str)]| {
                rows.iter().any(|(n, _, g)| n == name && g == group)
            };
            if !listed(&triples) && !listed(&extras) {
                extras.push((*name, label.clone(), *group));
            }
        }
    }
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
