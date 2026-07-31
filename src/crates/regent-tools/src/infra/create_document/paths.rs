//! Output-path placement for `create_document`: documents asked for together
//! land together, in one human-readable folder under the artifacts root.
//!
//! The folder is claimed by the FIRST document of a session (see
//! `ToolContext::document_folder`) and reused by every later one, because
//! nothing in a filename reliably says "these two belong together". A deck
//! called `..._Presentation.pptx` and its companion `..._Complete_Guide.pdf`
//! have different stems, so any name-derived rule splits them — which is
//! exactly what used to happen.
//!
//! An explicitly supplied subfolder always wins. That is the escape hatch for
//! a caller that is already organizing its own output.

use std::path::{Path, PathBuf};

/// Words that name the KIND of document rather than its subject. Stripped from
/// the end of the stem so the folder reads as the topic — `black-panther`, not
/// `black-panther-complete-guide`.
const TYPE_WORDS: &[&str] = &[
    "presentation",
    "deck",
    "slides",
    "slide",
    "guide",
    "complete",
    "report",
    "overview",
    "summary",
    "doc",
    "docs",
    "document",
    "sheet",
    "workbook",
    "final",
    "draft",
    "v1",
    "v2",
];

/// Where a document goes, relative to the artifacts root.
///
/// `session_folder` resolves the shared folder: it is handed this document's
/// own proposal and returns whatever the session actually settled on.
pub fn artifact_relative_path(
    path_str: &str,
    session_folder: impl FnOnce(&str) -> String,
) -> PathBuf {
    let path = Path::new(path_str);
    // Already inside a folder — the caller organized this deliberately.
    if path.components().count() != 1 {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("documents");
    PathBuf::from(session_folder(&folder_name(stem))).join(path)
}

/// A folder name from one document's stem: slugged, with trailing type words
/// removed. Never empty — a stem of pure punctuation still needs somewhere to
/// live.
pub fn folder_name(stem: &str) -> String {
    let slug = slug(stem);
    let mut words: Vec<&str> = slug.split('-').filter(|w| !w.is_empty()).collect();
    // Only from the END, and never all of them: `Presentation.pptx` is a
    // legitimate name whose only word is a type word, and stripping it would
    // leave nothing to name the folder with.
    while words.len() > 1 && TYPE_WORDS.contains(words.last().unwrap_or(&"")) {
        words.pop();
    }
    let name = words.join("-");
    if name.is_empty() {
        "documents".to_owned()
    } else {
        name
    }
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
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Take the proposal — what a session with no folder claimed yet does.
    fn first(proposed: &str) -> String {
        proposed.to_owned()
    }

    #[test]
    fn a_bare_name_gets_a_subject_folder() {
        assert_eq!(
            artifact_relative_path("Report.pdf", first),
            PathBuf::from("report").join("Report.pdf")
        );
    }

    /// The reported bug. Two documents from one request, different stems, and
    /// they must not end up in different places.
    #[test]
    fn a_deck_and_its_companion_pdf_share_one_folder() {
        let settled = std::cell::RefCell::new(None::<String>);
        let session = |proposed: &str| {
            let mut slot = settled.borrow_mut();
            slot.get_or_insert_with(|| proposed.to_owned()).clone()
        };
        let deck =
            artifact_relative_path("Black_Panther_Wakanda_Forever_Presentation.pptx", session);
        let pdf =
            artifact_relative_path("Black_Panther_Wakanda_Forever_Complete_Guide.pdf", session);
        assert_eq!(deck.parent(), pdf.parent(), "same folder");
        assert_eq!(
            deck.parent().unwrap(),
            Path::new("black-panther-wakanda-forever")
        );
        // The filenames are untouched — only the folder is decided here.
        assert_eq!(
            pdf.file_name().unwrap(),
            "Black_Panther_Wakanda_Forever_Complete_Guide.pdf"
        );
    }

    #[test]
    fn an_explicit_folder_is_left_alone() {
        // Not even consulted: passing a folder is how a caller opts out.
        let never = |_: &str| panic!("the session folder must not be consulted");
        assert_eq!(
            artifact_relative_path("q3/report.pdf", never),
            PathBuf::from("q3/report.pdf")
        );
    }

    #[test]
    fn type_words_are_stripped_from_the_end_only() {
        assert_eq!(folder_name("Wakanda_Report"), "wakanda");
        assert_eq!(folder_name("Wakanda_Complete_Guide_Final"), "wakanda");
        // Leading, not trailing — a report ABOUT guides keeps its subject.
        assert_eq!(folder_name("Guide_To_Wakanda"), "guide-to-wakanda");
    }

    #[test]
    fn a_name_that_is_only_a_type_word_still_gets_a_folder() {
        assert_eq!(folder_name("Presentation"), "presentation");
        assert_eq!(folder_name("---"), "documents");
    }
}
