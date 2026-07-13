//! Raw provider/core turn errors mapped to one actionable sentence.
//! Split from `prompt_ops.rs` (file-size rule).

/// Turn a raw provider/core turn error into one clear, actionable sentence for
/// the user — shown in chat and spoken on a call. The common, self-inflicted
/// causes (no credit, bad key, rate limit) get a specific fix; anything else
/// passes through as a short, non-JSON summary so the caller still hears why a
/// turn produced nothing instead of dead air.
pub(super) fn humanize_turn_error(raw: &str) -> String {
    let low = raw.to_lowercase();
    let has = |needle: &str| low.contains(needle);
    if has("402")
        || has("more credits")
        || has("insufficient")
        || has("out of credit")
        || has("can only afford")
    {
        return "Your AI provider is out of credits. Add credit to your provider account (for OpenRouter, top up at openrouter.ai) and try again.".into();
    }
    if has("401") || has("unauthorized") || has("invalid api key") || has("no auth credentials") {
        return "Your AI provider rejected the API key. Set a valid model provider key and try again.".into();
    }
    if has("429") || has("rate limit") || has("rate-limit") || has("too many requests") {
        return "Your AI provider is rate-limiting right now. Wait a few seconds and try again."
            .into();
    }
    // Any 404 is actionable: either the model id doesn't exist at the provider
    // or the provider entry's base_url points at a wrong path (the classic
    // symptom is an HTML error page instead of JSON).
    if has("404") || has("no endpoints found") || has("not a valid model") {
        return "The provider returned 404 — the model id or the provider's base_url is wrong. Check both in Settings → Model and try again.".into();
    }
    // Unknown: a trimmed, JSON-free summary so it's still legible when spoken.
    let brief: String = raw
        .split(&['{', '\n'][..])
        .next()
        .unwrap_or(raw)
        .trim()
        .chars()
        .take(160)
        .collect();
    format!("I couldn't reach the model. {brief}")
}

#[cfg(test)]
mod tests {
    use super::humanize_turn_error;

    #[test]
    fn credit_and_auth_errors_become_actionable_sentences() {
        let credit = humanize_turn_error(
            "core: provider failure: API error (HTTP 402): {\"error\":{\"message\":\"This request requires more credits, or fewer max_tokens. You requested up to 65536 tokens, but can only afford 31441\"}}",
        );
        assert!(credit.to_lowercase().contains("out of credits"), "{credit}");
        assert!(!credit.contains('{'), "no raw JSON when spoken: {credit}");

        assert!(
            humanize_turn_error("API error (HTTP 401): unauthorized")
                .to_lowercase()
                .contains("api key")
        );
        assert!(
            humanize_turn_error("HTTP 429: rate limit exceeded")
                .to_lowercase()
                .contains("rate-limiting")
        );
        // Unknown errors keep a short, JSON-free summary.
        let other = humanize_turn_error("core: some weird failure\n{\"detail\":1}");
        assert!(other.starts_with("I couldn't reach the model."), "{other}");
        assert!(!other.contains('{'), "{other}");
    }
}
