//! Output-path placement for `create_document`: a bare deck filename gets its
//! own human-readable folder so decks don't collide in a shared artifact root.

use super::model::DocFormat;
use std::path::{Path, PathBuf};

/// A bare deck filename is ambiguous in a shared artifact root. Put it in a
/// deterministic, human-readable folder while preserving explicitly supplied
/// subfolders for callers that already organize their output.
pub fn artifact_relative_path(format: DocFormat, path_str: &str) -> PathBuf {
    let path = Path::new(path_str);
    if format != DocFormat::Pptx || path.components().count() != 1 {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("presentation");
    PathBuf::from(slug(stem)).join(path)
}

fn slug(value: &str) -> String {
    let mut out = String::new();
    let mut separator = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            if separator && !out.is_empty() {
                out.push('-');
            }
            out.push(ch.to_ascii_lowercase());
            separator = false;
        } else {
            separator = true;
        }
    }
    if out.is_empty() {
        "presentation".to_owned()
    } else {
        out
    }
}
