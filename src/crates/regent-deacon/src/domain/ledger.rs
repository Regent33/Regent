//! Stable-Prefix Ledger (SPL P1 — `docs/proposal/token-efficiency-architecture-v1.md`
//! §3.1/§3.3). The prompt's fixed prefix is worth 2–10× less when providers can
//! serve it from cache, but only while it stays byte-stable — and one careless
//! per-turn `format!` silently doubles the bill forever. The Ledger is the one
//! type allowed to concatenate the system prompt: each segment carries a
//! stability tier, the build-time render is the baseline, and a cheap per-turn
//! check re-hashes what is actually sent so a cache-busting regression is
//! caught on its first affected turn instead of on a bill months later.
//!
//! Pure and fail-open by construction: no I/O, no errors — a mismatch is a
//! report (`Bust`), never a failed turn. Hashes use `DefaultHasher`; they are
//! only ever compared within one process lifetime.

use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;

/// Stability tier of a prompt segment. Only the two hashed tiers are modeled:
/// Tier 2 (history) is owned by the transcript and Tier 3 (volatile) is
/// per-turn by definition — neither is part of the stable prefix P1 guards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Tier 0 PROCESS — SYSTEM_PROMPT, CAPABILITIES, env-derived lines (env is
    /// read once at spawn), and the serialized tool catalog. Must not change
    /// for the life of the process.
    Process,
    /// Tier 1 SESSION — persona block, skills index, graph memory block.
    /// Frozen at session build; live edits reach the wire only at the next
    /// session build (mid-session deltas ride Tier 3, P2+).
    Session,
}

impl Tier {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Tier::Process => "tier0",
            Tier::Session => "tier1",
        }
    }
}

/// One named span of the rendered system prompt. Separator bytes ("\n\n")
/// belong to the segment they precede, so concatenating segments in order is
/// byte-identical to the historical `format!` assembly — changing that
/// invalidates every frozen session prompt and P0's measurements.
#[derive(Debug, Clone)]
pub struct Segment {
    pub name: &'static str,
    pub tier: Tier,
    pub text: String,
}

impl Segment {
    #[must_use]
    pub fn tier0(name: &'static str, text: impl Into<String>) -> Self {
        Self {
            name,
            tier: Tier::Process,
            text: text.into(),
        }
    }

    #[must_use]
    pub fn tier1(name: &'static str, text: impl Into<String>) -> Self {
        Self {
            name,
            tier: Tier::Session,
            text: text.into(),
        }
    }
}

/// A detected mid-session change to a stable-prefix component — the thing the
/// `cache_bust` warning names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bust {
    pub tier: Tier,
    pub segment: &'static str,
}

/// Build-time baseline of a session's stable prefix: the ordered segments plus
/// hashes of the rendered prompt and the serialized tool definitions.
#[derive(Debug, Clone)]
pub struct Ledger {
    segments: Vec<Segment>,
    prompt_hash: u64,
    /// Hash of the tool definitions exactly as serialized for the provider.
    /// `None` until `seal` — the definitions are final only after the caller's
    /// disable/defer/restrict passes, which happen after segment assembly.
    defs_hash: Option<u64>,
}

impl Ledger {
    #[must_use]
    pub fn new(segments: Vec<Segment>) -> Self {
        let prompt_hash = hash_str(&render(&segments));
        Self {
            segments,
            prompt_hash,
            defs_hash: None,
        }
    }

    /// The system prompt — the segments concatenated in order, nothing else.
    #[must_use]
    pub fn render(&self) -> String {
        render(&self.segments)
    }

    /// Records the tool-definitions baseline (call once the catalog is final).
    pub fn seal(&mut self, serialized_defs: &str) {
        self.defs_hash = Some(hash_str(serialized_defs));
    }

    /// Resume keeps the STORED prompt when it differs from a fresh render
    /// (byte-stability across resumes — regent-agent's rule). Tier attribution
    /// inside a foreign stored prompt is unknowable, so the ledger collapses to
    /// one session-tier segment: stability checking still works, attribution
    /// just coarsens. No-op when the agent uses the rendered prompt.
    pub fn rebase(&mut self, actual_prompt: &str) {
        if hash_str(actual_prompt) == self.prompt_hash {
            return;
        }
        self.segments = vec![Segment::tier1("stored_prompt", actual_prompt)];
        self.prompt_hash = hash_str(actual_prompt);
    }

    /// Build-time per-tier hashes as hex, `(tier0, tier1)` — the additive
    /// `turn.complete` fields clients watch for cross-turn stability. Tier 0
    /// folds in the tool-definitions hash: the catalog rides the same cache
    /// prefix as the process-stable prompt text.
    #[must_use]
    pub fn tier_hashes_hex(&self) -> (String, String) {
        let mut t0 = DefaultHasher::new();
        let mut t1 = DefaultHasher::new();
        t0.write_u64(self.defs_hash.unwrap_or(0));
        for seg in &self.segments {
            match seg.tier {
                Tier::Process => t0.write(seg.text.as_bytes()),
                Tier::Session => t1.write(seg.text.as_bytes()),
            }
        }
        (
            format!("{:016x}", t0.finish()),
            format!("{:016x}", t1.finish()),
        )
    }

    /// Compares what a turn will actually send — the agent's frozen prompt
    /// string and the freshly re-serialized tool definitions — against the
    /// build baseline. Pure so tests can assert trips without capturing logs;
    /// the caller turns each `Bust` into a `cache_bust` warning.
    #[must_use]
    pub fn check(&self, current_prompt: &str, current_serialized_defs: &str) -> Vec<Bust> {
        let mut busts = Vec::new();
        // Re-serializing every turn is the point: it catches serialization
        // instability (a HashMap sneaking into definitions) that a stored
        // string could never show.
        if self
            .defs_hash
            .is_some_and(|h| h != hash_str(current_serialized_defs))
        {
            busts.push(Bust {
                tier: Tier::Process,
                segment: "tool_definitions",
            });
        }
        if hash_str(current_prompt) == self.prompt_hash {
            return busts;
        }
        // Attribute by walking the stored segments at their byte offsets. The
        // FIRST mismatching segment names the busted tier; offsets after a
        // length change are unreliable, so stop there.
        let bytes = current_prompt.as_bytes();
        let mut offset = 0usize;
        for seg in &self.segments {
            let end = offset + seg.text.len();
            let unchanged = bytes
                .get(offset..end)
                .is_some_and(|slice| slice == seg.text.as_bytes());
            if !unchanged {
                busts.push(Bust {
                    tier: seg.tier,
                    segment: seg.name,
                });
                return busts;
            }
            offset = end;
        }
        // Every segment intact but the prompt grew: content appended past the
        // stable prefix — a per-turn injection that belongs in Tier 3.
        busts.push(Bust {
            tier: Tier::Session,
            segment: "trailing_injection",
        });
        busts
    }
}

fn render(segments: &[Segment]) -> String {
    let mut out = String::with_capacity(segments.iter().map(|s| s.text.len()).sum());
    for seg in segments {
        out.push_str(&seg.text);
    }
    out
}

fn hash_str(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    h.write(s.as_bytes());
    h.finish()
}
