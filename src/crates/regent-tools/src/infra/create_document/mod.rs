//! `create_document` — generate PDF / Word / PowerPoint / Excel files natively
//! in-process, the write-side twin of `read_document`. Same motivation: with no
//! document tool the model fell back to `python3 -c` one-liners that hang on
//! Windows. One flat content spec (`model::DocumentSpec`) drives all four
//! writers; `format` picks which of `sections` / `slides` / `sheets` is
//! authoritative. This module is the executor: it resolves + jails the output
//! path, loads content (create or edit), hydrates images, then hands off to
//! `synth` for the bytes and writes them once. Schema lives in `schema`, byte
//! synthesis in `synth`, path placement in `paths`.

mod deck;
mod docx;
mod edit;
mod execute;
mod html;
mod images;
mod model;
mod palette;
mod paths;
mod pdf;
mod pptx;
mod pptx_scaffold;
mod pptx_shapes;
mod pptx_slide;
mod pptx_xml;
mod preview;
mod renderer;
mod schema;
mod synth;
mod theme;
mod validate;
mod xlsx;

use crate::ToolCatalog;
use execute::CreateDocumentTool;
use regent_kernel::RegentError;
use std::sync::Arc;

#[cfg(test)]
use crate::domain::contracts::ToolExecutor;
#[cfg(test)]
use crate::domain::entities::ToolContext;
#[cfg(test)]
use model::DocumentSpec;
#[cfg(test)]
use serde_json::{Value, json};

/// Registers `create_document` on the catalog.
pub fn register_create_document_tool(catalog: &mut ToolCatalog) -> Result<(), RegentError> {
    catalog.register(schema::definition(), Arc::new(CreateDocumentTool))
}

#[cfg(test)]
#[path = "tests/support.rs"]
mod tests_support;

#[cfg(test)]
#[path = "tests/images.rs"]
mod image_tests;

#[cfg(test)]
#[path = "tests/edit.rs"]
mod edit_tests;

#[cfg(test)]
#[path = "tests/round_trip.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/rejects.rs"]
mod rejects_tests;
