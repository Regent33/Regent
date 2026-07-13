//! The agent's prompt layers, separated by role (pure data — no I/O):
//! - [`SYSTEM_PROMPT`] — behavior/identity preamble, shared by every surface.
//! - [`CONSTITUTIONAL_PROMPT`] — the opt-in values layer (character + hard
//!   boundaries), shipped as a versioned document and seeded into the
//!   `constitution` persona row at setup (see the deacon composition root).
//! - [`CAPABILITIES`] — the command-surface reference, hand-maintained to
//!   match the CLI router.
//! - [`CODING_PROMPT`] — the coding-work overlay the `regent-code` harness
//!   prepends to the surface prompt for both phases.

mod coding;
mod constitution;
mod system;

pub use coding::{CODING_PROMPT, EXPLORE_PROMPT, WRAP_UP_PROMPT};
pub use constitution::{
    CONSTITUTIONAL_PROMPT, ConstitutionSection, constitution_chunks, constitution_core,
    constitution_sections, constitution_text,
};
pub use system::{CAPABILITIES, SYSTEM_PROMPT, VISUAL_EXPLAINER};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_layers_are_distinct_and_non_empty() {
        assert!(!SYSTEM_PROMPT.is_empty());
        assert!(!CAPABILITIES.is_empty());
        // The layers must stay separable — no layer embeds another.
        assert!(!SYSTEM_PROMPT.contains("## Your commands"));
        assert!(!CAPABILITIES.contains("You are Regent by default"));
    }

    /// Butler regression (2026-07-13): "create a code task" on a call must be
    /// an ACTION, never a diagram — the visual-first rules carry an explicit
    /// work-request override naming the action tools.
    #[test]
    fn visual_explainer_never_lets_a_diagram_replace_work() {
        assert!(VISUAL_EXPLAINER.contains("WORK requests"));
        assert!(VISUAL_EXPLAINER.contains("code_task"));
        assert!(VISUAL_EXPLAINER.contains("NEVER answer a work request with a diagram"));
    }
}
