//! Composition helpers: explicit manifests of which executors back which
//! definitions. Registration is deliberate — never an import side effect.

use crate::application::catalog::ToolCatalog;
use crate::domain::contracts::TerminalBackend;
use crate::infra::backends::{LocalBackend, terminal_backend_from_env};
use crate::infra::{
    apply_patch, camera, computer_use, control_app, file_edit, files, glob, image_generation, ls,
    play, read_document, search, terminal, time_tool, video_analyze, vision_analyze, web_search,
};
use regent_kernel::RegentError;
use std::sync::Arc;

/// The default core toolset (the "narrow waist" — every entry here is paid
/// for on every API call, so additions go up the Footprint Ladder first).
///
/// Uses the host `local` terminal backend; composition roots that serve
/// untrusted input should prefer [`core_catalog_from_env`] so
/// `REGENT_TERMINAL_BACKEND`/`REGENT_SANDBOX` take effect.
#[must_use]
pub fn core_catalog() -> ToolCatalog {
    core_catalog_with_terminal(Arc::new(LocalBackend))
}

/// Core toolset with the terminal backend selected from the environment
/// (`REGENT_TERMINAL_BACKEND`), enforcing `REGENT_SANDBOX` (which forbids the
/// host `local` backend). This is the wiring that makes the docker/ssh/sandbox
/// backends actually reachable.
pub fn core_catalog_from_env() -> Result<ToolCatalog, RegentError> {
    Ok(core_catalog_with_terminal(terminal_backend_from_env()?))
}

/// Core toolset with a chosen terminal backend (docker/ssh sandboxes).
#[must_use]
pub fn core_catalog_with_terminal(backend: Arc<dyn TerminalBackend>) -> ToolCatalog {
    let mut catalog = ToolCatalog::new();
    // Registration of built-ins cannot collide; expect() documents that.
    catalog
        .register(
            terminal::definition(),
            Arc::new(terminal::TerminalTool::with_backend(backend)),
        )
        .expect("core tool 'terminal' registers once");
    catalog
        .register(files::read_definition(), Arc::new(files::ReadFileTool))
        .expect("core tool 'read_file' registers once");
    read_document::register_read_document_tool(&mut catalog)
        .expect("core tool 'read_document' registers once");
    catalog
        .register(files::write_definition(), Arc::new(files::WriteFileTool))
        .expect("core tool 'write_file' registers once");
    catalog
        .register(file_edit::definition(), Arc::new(file_edit::FileEditTool))
        .expect("core tool 'file_edit' registers once");
    catalog
        .register(
            apply_patch::definition(),
            Arc::new(apply_patch::ApplyPatchTool),
        )
        .expect("core tool 'apply_patch' registers once");
    catalog
        .register(search::definition(), Arc::new(search::SearchFilesTool))
        .expect("core tool 'search_files' registers once");
    catalog
        .register(glob::definition(), Arc::new(glob::GlobTool))
        .expect("core tool 'glob' registers once");
    catalog
        .register(ls::definition(), Arc::new(ls::LsTool))
        .expect("core tool 'ls' registers once");
    catalog
        .register(
            time_tool::definition(),
            Arc::new(time_tool::CurrentTimeTool),
        )
        .expect("core tool 'current_time' registers once");
    catalog
        .register(
            web_search::search_definition(),
            Arc::new(web_search::WebSearchTool),
        )
        .expect("core tool 'web_search' registers once");
    catalog
        .register(
            web_search::fetch_definition(),
            Arc::new(web_search::WebFetchTool),
        )
        .expect("core tool 'web_fetch' registers once");
    catalog
        .register(
            vision_analyze::definition(),
            Arc::new(vision_analyze::VisionAnalyzeTool),
        )
        .expect("core tool 'vision_analyze' registers once");
    camera::register_camera_tool(&mut catalog).expect("core tool 'camera_capture' registers once");
    catalog
        .register(
            image_generation::definition(),
            Arc::new(image_generation::ImageGenerationTool),
        )
        .expect("core tool 'image_generation' registers once");
    catalog
        .register(
            video_analyze::definition(),
            Arc::new(video_analyze::VideoAnalyzeTool),
        )
        .expect("core tool 'video_analyze' registers once");
    // High-privilege desktop control — only present when explicitly enabled
    // (REGENT_COMPUTER_USE=1); every mutating action is approval-gated.
    if computer_use::is_enabled() {
        catalog
            .register(
                computer_use::definition(),
                Arc::new(computer_use::ComputerUseTool::new(
                    computer_use::default_backend(),
                )),
            )
            .expect("core tool 'computer_use' registers once");
    }
    catalog
        .register(
            control_app::definition(),
            Arc::new(control_app::ControlAppTool),
        )
        .expect("core tool 'control_app' registers once");
    catalog
        .register(play::definition(), Arc::new(play::PlayTool))
        .expect("core tool 'play' registers once");
    catalog
}
