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
    // A 404 whose body names the account's data policy is NOT a wrong id, and
    // must be caught before the generic 404 rung below (the body carries both
    // "404" and the endpoint-count wording that rung matches). OpenRouter
    // serves ":free" tiers only to accounts that opt into training on their
    // prompts, and returns 404 — "0 endpoints ... matching your guardrail
    // restrictions and data policy" — to everyone else. Sending that user to
    // Settings → Model tells them to fix an id that is already correct, which
    // is how one of these cost a debugging session.
    if has("data policy") || has("free model training") || has("settings/privacy") {
        return "Your OpenRouter privacy settings block this model — \":free\" tiers train on your prompts and are off by default. Either enable free-model training at openrouter.ai/settings/privacy, or drop the \":free\" suffix in Settings → Model to use the paid tier.".into();
    }
    // Any other 404 is actionable: either the model id doesn't exist at the
    // provider or the provider entry's base_url points at a wrong path (the
    // classic symptom is an HTML error page instead of JSON).
    if has("404") || has("no endpoints found") || has("not a valid model") {
        return "The provider returned 404 — the model id or the provider's base_url is wrong. Check both in Settings → Model and try again.".into();
    }
    // A request timeout: the per-request budget elapsed before the model
    // answered — usually a slow first token on a long prompt, sometimes a hung
    // endpoint. The provider layer emits stable "request timed out" text, so
    // this stays robust across reqwest versions.
    if has("timed out") || has("timeout") {
        return "The AI provider took too long and the request timed out — often a slow model on a long prompt. Try again, or switch to a faster model in Settings → Model.".into();
    }
    // Transport couldn't reach the provider at all (connection refused, DNS).
    if has("could not connect") || has("connection refused") || has("dns error") {
        return "I couldn't connect to your AI provider. Check your internet connection and the provider's base URL in Settings → Model, then try again.".into();
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

    #[test]
    fn a_data_policy_404_is_not_reported_as_a_wrong_model_id() {
        // The real OpenRouter body for a ":free" tier on an account that has
        // not opted into training. It contains "404" and the endpoint wording,
        // so it would fall into the generic rung if ordering ever regressed.
        let raw = "core: provider failure: API error (HTTP 404): {\"error\":{\"message\":\"0 endpoints out of 1 requested are available matching your guardrail restrictions and data policy. We removed them for the following reasons (an endpoint may have matched multiple reasons):\nFree model training violation (account settings): 1 endpoint excluded; configurable at https://openrouter.ai/settings/privacy\"}}";
        let msg = humanize_turn_error(raw);
        assert!(msg.contains("privacy"), "{msg}");
        assert!(
            !msg.contains("base_url"),
            "a data-policy 404 must not be blamed on the model id: {msg}"
        );
        assert!(!msg.contains('{'), "no raw JSON when spoken: {msg}");
        // A genuinely wrong id still gets the id/base_url sentence.
        let wrong = humanize_turn_error("API error (HTTP 404): not a valid model id");
        assert!(wrong.contains("base_url"), "{wrong}");
    }

    #[test]
    fn timeout_and_connection_errors_are_actionable_and_leak_no_raw_text() {
        // The stable text the provider layer now emits on a total-request
        // timeout (the Nemotron long-prefill case).
        let timeout =
            humanize_turn_error("core: provider failure: network error: request timed out");
        assert!(timeout.to_lowercase().contains("timed out"), "{timeout}");
        assert!(
            !timeout.contains("network error"),
            "raw provider text must not leak: {timeout}"
        );
        assert!(
            !timeout.starts_with("I couldn't reach the model."),
            "{timeout}"
        );

        let connect = humanize_turn_error(
            "core: provider failure: network error: could not connect to the provider",
        );
        assert!(connect.to_lowercase().contains("connect"), "{connect}");
        assert!(!connect.contains("network error"), "{connect}");
    }
}
