//! Capability env defaults for the standalone gateway binary.
//!
//! Every other Regent front-end defaults `REGENT_COMPUTER_USE=1` in its
//! *spawner* — the CLI (`spawn.ts`), the desktop app (`spawn.rs`), the voice
//! server — so chat gets screen/desktop control with mutating actions still
//! approval-gated. The gateway has no spawner it owns: it is the long-lived
//! process itself, and it can be started any number of ways —
//! `regent gateway start`, a Windows Startup `.cmd`, a systemd unit, a bare
//! `./regent-gateway`. If whichever launcher started it forgets the flag, the
//! chat session silently loses `computer_use` and answers "I can't see the
//! screen" — the exact failure users hit, and a textbook
//! composition-root-drift bug (the gateway is a second composition root).
//!
//! So the gateway defaults its own capabilities *in-process*, independent of
//! how it was launched. Belt-and-suspenders with the CLI's `gatewayEnv.ts`,
//! and the only version that survives a launcher that never runs that code.

/// Env vars the gateway turns on by default, each `(name, default_value)`.
/// Applied only when the var is **unset** — an explicit value always wins,
/// including `"0"` to deliberately disable. Data, not code, so a future
/// default-on capability is one row here plus one test row.
pub const CAPABILITY_DEFAULTS: &[(&str, &str)] = &[
    // Desktop control (screenshot / click / type). Every mutating action stays
    // approval-gated — over chat that is the `/approve` reply — while a
    // read-only screenshot is ungated. Matches every other Regent surface, so
    // "can you see my screen?" behaves the same whether asked over chat, the
    // desktop app, the CLI, or a voice call.
    ("REGENT_COMPUTER_USE", "1"),
];

/// Pure core: given a way to read the current environment, decide which
/// defaults still need setting. Any already-present var (any value, including
/// empty) is left untouched. Split out from the process mutation below so the
/// decision is testable without touching global state.
#[must_use]
pub fn pending_defaults(
    defaults: &[(&'static str, &'static str)],
    mut lookup: impl FnMut(&str) -> Option<String>,
) -> Vec<(&'static str, &'static str)> {
    defaults
        .iter()
        .copied()
        .filter(|pair| lookup(pair.0).is_none())
        .collect()
}

/// Apply [`CAPABILITY_DEFAULTS`] to the process environment.
///
/// # Ordering (why this is only called from `main`)
///
/// It writes the process environment via `std::env::set_var`, which on edition
/// 2024 is `unsafe`: a write must not race another thread reading the
/// environment. Call it as the **first** thing in `main`, before the Tokio
/// runtime (or any other thread) is created. Everything downstream —
/// `run()`, each session's `build_agent`, `computer_use::is_enabled()` — then
/// only *reads* the flag, from worker threads, with no concurrent writer.
///
/// After this returns, `is_enabled()` reflects the resolved value process-wide,
/// so it also reaches delegated / background sub-agents built from the same
/// process.
pub fn apply_capability_defaults() {
    for (name, value) in pending_defaults(CAPABILITY_DEFAULTS, |key| std::env::var(key).ok()) {
        // SAFETY: called before the async runtime starts (see the ordering
        // note above) — single-threaded, so no reader can race this write.
        unsafe { std::env::set_var(name, value) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_is_a_capability_the_gateway_should_own() {
        // The roster is what makes chat a full Regent; if it ever empties, the
        // in-process default has silently become a no-op.
        assert!(
            CAPABILITY_DEFAULTS
                .iter()
                .any(|(k, v)| *k == "REGENT_COMPUTER_USE" && *v == "1"),
            "computer_use must default on for the gateway"
        );
    }

    #[test]
    fn an_unset_capability_is_scheduled_to_be_defaulted_on() {
        let pending = pending_defaults(CAPABILITY_DEFAULTS, |_| None);
        assert_eq!(pending, CAPABILITY_DEFAULTS.to_vec());
    }

    #[test]
    fn an_explicit_value_is_never_overridden() {
        // Both an explicit "0" (user turned it off) and "1" (already on) must
        // be left alone — the gateway only fills in what the launcher omitted.
        // Empty string counts as set too: it disables, and that stays the
        // user's choice.
        for existing in ["0", "1", ""] {
            let pending = pending_defaults(CAPABILITY_DEFAULTS, |_| Some(existing.to_owned()));
            assert!(pending.is_empty(), "explicit {existing:?} was overridden");
        }
    }

    #[test]
    fn only_the_missing_vars_are_returned_from_a_mixed_roster() {
        const ROSTER: &[(&str, &str)] = &[("SET_ONE", "a"), ("MISSING_ONE", "b")];
        let pending = pending_defaults(ROSTER, |key| {
            (key == "SET_ONE").then(|| "present".to_owned())
        });
        assert_eq!(pending, vec![("MISSING_ONE", "b")]);
    }
}
