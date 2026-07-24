//! The agent's prompt layers, separated by role (pure data — no I/O):
//! - [`SYSTEM_PROMPT`] — behavior/identity preamble, shared by every surface.
//! - [`CONSTITUTIONAL_PROMPT`] — the opt-in values layer (character + hard
//!   boundaries), shipped as a versioned document and seeded into the
//!   `constitution` persona row at setup (see the deacon composition root).
//! - [`CAPABILITIES`] — the command-surface reference, hand-maintained to
//!   match the CLI router.
//! - [`CODING_PROMPT`] — the coding-work overlay the `regent-code` harness
//!   prepends to the surface prompt for both phases.

mod capabilities;
mod coding;
mod constitution;
mod system;

pub use capabilities::CAPABILITIES;
pub use coding::{CODING_PROMPT, EXPLORE_PROMPT, WRAP_UP_PROMPT};
pub use constitution::{
    CONSTITUTIONAL_PROMPT, ConstitutionSection, constitution_chunks, constitution_core,
    constitution_sections, constitution_text, legacy_constitution_cores,
};
pub use system::{
    SYSTEM_PROMPT, SYSTEM_PROMPT_SCHEMA_MARKER, VISUAL_EXPLAINER, system_prompt_schema,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_layers_are_distinct_and_non_empty() {
        assert!(!SYSTEM_PROMPT.is_empty());
        assert_eq!(
            system_prompt_schema(SYSTEM_PROMPT),
            Some(SYSTEM_PROMPT_SCHEMA_MARKER)
        );
        let light = format!("profile: light\n\n{SYSTEM_PROMPT}");
        assert_eq!(
            system_prompt_schema(&light),
            Some(SYSTEM_PROMPT_SCHEMA_MARKER),
            "light-profile sessions must still rebase on schema bumps"
        );
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

    /// Voice incident (2026-07-23): "explain Ferrari vs Lamborghini" was
    /// answered by building a title-only PPTX via create_document instead of the
    /// inline compare diagram. The explainer must route explanation / comparison
    /// / overview / history to the inline diagram and forbid the file tools
    /// unless the user explicitly asks for a file — and the bumped schema marker
    /// (v4) makes resumed v3 sessions rebase onto this wording.
    #[test]
    fn visual_explainer_keeps_explanations_inline_not_a_deck() {
        assert_eq!(SYSTEM_PROMPT_SCHEMA_MARKER, "regent-prompt-schema:v4");
        assert!(VISUAL_EXPLAINER.contains("answered INLINE"));
        assert!(VISUAL_EXPLAINER.contains("do NOT call create_document, background_task"));
        assert!(VISUAL_EXPLAINER.contains("UNLESS the user EXPLICITLY asks for a file"));
    }

    #[test]
    fn explicit_actions_require_a_completed_tool_action_on_every_surface() {
        assert!(SYSTEM_PROMPT.contains("EXECUTE EXPLICIT ACTIONS"));
        assert!(SYSTEM_PROMPT.contains("search results alone do not open anything"));
        assert!(SYSTEM_PROMPT.contains("File Explorer"));
        assert!(VISUAL_EXPLAINER.contains("web_search is only discovery"));
        assert!(VISUAL_EXPLAINER.contains("call open_url"));
    }
}
