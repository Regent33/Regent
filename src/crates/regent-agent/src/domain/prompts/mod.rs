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
        assert_eq!(SYSTEM_PROMPT_SCHEMA_MARKER, "regent-prompt-schema:v5");
        assert!(VISUAL_EXPLAINER.contains("answered INLINE"));
        assert!(VISUAL_EXPLAINER.contains("do NOT call create_document, background_task"));
        assert!(VISUAL_EXPLAINER.contains("UNLESS the user EXPLICITLY asks for a file"));
    }

    /// Butler incident (2026-08-12, `sess_7be9938118bc43ab9807135dd0fce383`):
    /// after 15 tool calls the model replied *"Now I have a solid foundation
    /// from multiple authoritative sources. Let me present a clear visual
    /// timeline…"* — and emitted no block at all. The user watched an empty
    /// screen while Regent talked about the diagram it never drew.
    ///
    /// The prompt caused it. Requirement (1) said lead with the block, while
    /// (2) called it "natural (encouraged)" to cue the visual — without saying
    /// the cue comes AFTER. The natural reading of "cue it" is to say it first,
    /// so the model announced, and treated announcing as done. A working turn
    /// in the same period shows the same shape landing better by luck:
    /// *"Here's the timeline on screen."* THEN the block — still prose-first,
    /// so the picture arrived after the talking had already started.
    #[test]
    fn the_diagram_block_precedes_every_word_including_its_own_announcement() {
        assert!(VISUAL_EXPLAINER.contains("the very first character you emit"));
        assert!(VISUAL_EXPLAINER.contains("Announcing a visual is NOT producing one"));
        assert!(VISUAL_EXPLAINER.contains("the block IS the presenting"));
        // The cue is still allowed — but only once the block is out.
        assert!(VISUAL_EXPLAINER.contains("AFTER the block has been emitted"));
        // ISSUE 1 guard (the "Butler drew a diagram for 'oh'" bleed, 76e7d6d).
        // Making the ordering rule emphatic is exactly the edit that could
        // reintroduce it, so the whether/where split is pinned here: the
        // decision comes first, and the ordering rule is explicitly barred from
        // creating a diagram the decision refused.
        assert!(VISUAL_EXPLAINER.contains("TWO STEPS"));
        assert!(VISUAL_EXPLAINER.contains("decide WHETHER"));
        assert!(VISUAL_EXPLAINER.contains("Most turns have not"));
        assert!(VISUAL_EXPLAINER.contains("gets SPEECH ONLY and no block"));
        assert!(VISUAL_EXPLAINER.contains("never about whether to draw one"));
        assert!(
            VISUAL_EXPLAINER.contains("it can never create one that step one refused"),
            "the ordering rule must not be readable as 'always draw'"
        );
        // …and the user explicitly asking still short-circuits to the fast path.
        assert!(VISUAL_EXPLAINER.contains("step one is already satisfied"));
        // Resumed sessions keep their STORED prompt, so wording changes reach
        // them only when the marker moves. It shipped with v4; without this
        // bump every live butler session would have kept the wording that
        // caused the incident.
        assert_eq!(SYSTEM_PROMPT_SCHEMA_MARKER, "regent-prompt-schema:v5");
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
