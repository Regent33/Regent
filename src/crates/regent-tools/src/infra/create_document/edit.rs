//! Regent-generated documents carry a `<file>.regent.json` manifest holding the
//! exact content spec that produced them. `operation: "edit"` reloads that
//! manifest, applies an RFC 7386 JSON Merge Patch from the request's `patch`
//! field, and re-renders — so the model can tweak a deck it made without
//! restating the whole thing. Third-party files have no manifest; those go
//! through `read_document` + a fresh `create` instead.

use serde_json::Value;
use std::path::{Path, PathBuf};

/// Sidecar manifest path for a generated file: `<file>.regent.json` beside it
/// (e.g. `deck.pptx` -> `deck.pptx.regent.json`). Appending rather than
/// replacing the extension keeps it unambiguous and never collides with the
/// document itself.
pub fn manifest_path(document: &Path) -> PathBuf {
    let mut name = document
        .file_name()
        .map(std::ffi::OsStr::to_owned)
        .unwrap_or_default();
    name.push(".regent.json");
    document.with_file_name(name)
}

/// Applies an RFC 7386 JSON Merge Patch to `target` in place: objects merge
/// recursively, a JSON `null` in the patch deletes that key, and every other
/// value (including arrays) replaces wholesale. That is the entire spec — it is
/// deliberately shallow on arrays, so editing a slide list means resupplying the
/// whole `slides` array, not indexing into it.
pub fn merge_patch(target: &mut Value, patch: &Value) {
    let Value::Object(patch_map) = patch else {
        *target = patch.clone();
        return;
    };
    if !target.is_object() {
        *target = Value::Object(serde_json::Map::new());
    }
    let target_map = target.as_object_mut().expect("just forced to object");
    for (key, sub) in patch_map {
        if sub.is_null() {
            target_map.remove(key);
        } else {
            merge_patch(target_map.entry(key).or_insert(Value::Null), sub);
        }
    }
}

/// Strips the control fields (`operation`, `patch`) so what we persist to the
/// manifest and deserialize into a `DocumentSpec` is pure content.
pub fn content_only(mut value: Value) -> Value {
    if let Value::Object(map) = &mut value {
        map.remove("operation");
        map.remove("patch");
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn manifest_sits_beside_the_document() {
        let path = manifest_path(Path::new("/out/deck.pptx"));
        assert_eq!(path, PathBuf::from("/out/deck.pptx.regent.json"));
    }

    #[test]
    fn merge_recurses_objects_and_replaces_arrays() {
        let mut target = json!({
            "title": "Old",
            "meta": {"author": "A", "draft": true},
            "slides": [{"title": "S1"}]
        });
        merge_patch(
            &mut target,
            &json!({
                "title": "New",
                "meta": {"draft": null},
                "slides": [{"title": "S1b"}, {"title": "S2"}]
            }),
        );
        assert_eq!(
            target,
            json!({
                "title": "New",
                "meta": {"author": "A"},
                "slides": [{"title": "S1b"}, {"title": "S2"}]
            })
        );
    }

    #[test]
    fn non_object_patch_replaces_wholesale() {
        let mut target = json!({"a": 1});
        merge_patch(&mut target, &json!("scalar"));
        assert_eq!(target, json!("scalar"));
    }

    #[test]
    fn content_only_drops_control_fields() {
        let stripped = content_only(json!({
            "format": "pdf", "path": "x.pdf",
            "operation": "edit", "patch": {"title": "T"}
        }));
        assert_eq!(stripped, json!({"format": "pdf", "path": "x.pdf"}));
    }
}
