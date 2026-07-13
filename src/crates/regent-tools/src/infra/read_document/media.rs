//! Non-text payload of an OOXML document: embedded media files and hyperlink
//! targets. Documents are rarely text-only — a deck can carry its content in
//! pictures, and links are invisible in stripped text (the rels XML holds
//! them). Extraction never fails the read: any error degrades to "no media".

use std::io::Read;
use std::path::Path;

/// Cap on extracted images per document — a 200-slide deck of photos should
/// not flood the scratch area (the first N carry the shape).
const MAX_IMAGES: usize = 40;

/// Extracts embedded media to `<media_dir>/<file-stem>-media/` and collects
/// hyperlink targets from every `.rels` part. Returns (image paths, links).
pub(super) fn media_and_links(path: &Path, media_dir: Option<&Path>) -> (Vec<String>, Vec<String>) {
    let Ok(file) = std::fs::File::open(path) else {
        return (Vec::new(), Vec::new());
    };
    let Ok(mut archive) = zip::ZipArchive::new(file) else {
        return (Vec::new(), Vec::new());
    };

    let names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_owned()))
        .collect();

    let mut links: Vec<String> = Vec::new();
    for name in names.iter().filter(|n| n.ends_with(".rels")) {
        let mut xml = String::new();
        if archive
            .by_name(name)
            .ok()
            .and_then(|mut e| e.read_to_string(&mut xml).ok())
            .is_none()
        {
            continue;
        }
        links.extend(hyperlink_targets(&xml));
    }
    links.sort();
    links.dedup();

    let mut images: Vec<String> = Vec::new();
    if let Some(dir) = media_dir {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("document");
        let out_dir = dir.join(format!("{stem}-media"));
        for name in names.iter().filter(|n| is_media(n)).take(MAX_IMAGES) {
            let mut bytes = Vec::new();
            if archive
                .by_name(name)
                .ok()
                .and_then(|mut e| e.read_to_end(&mut bytes).ok())
                .is_none()
            {
                continue;
            }
            let base = name.rsplit('/').next().unwrap_or(name);
            let target = out_dir.join(base);
            match std::fs::create_dir_all(&out_dir).and_then(|()| std::fs::write(&target, &bytes)) {
                Ok(()) => images.push(target.display().to_string()),
                Err(error) => {
                    tracing::warn!(%error, media = name, "embedded media extraction failed");
                }
            }
        }
    }
    (images, links)
}

/// Media parts live under `word/media/`, `ppt/media/`, or `xl/media/`.
fn is_media(name: &str) -> bool {
    ["word/media/", "ppt/media/", "xl/media/"]
        .iter()
        .any(|p| name.starts_with(p))
}

/// `Target="…"` of every hyperlink `<Relationship>` in a rels part. External
/// links only — internal anchors (`TargetMode` absent, relative targets) are
/// navigation noise, not content.
fn hyperlink_targets(xml: &str) -> Vec<String> {
    xml.split("<Relationship")
        .filter(|chunk| chunk.contains("/hyperlink\""))
        .filter_map(|chunk| {
            let start = chunk.find("Target=\"")? + "Target=\"".len();
            let end = chunk[start..].find('"')? + start;
            let target = &chunk[start..end];
            target.starts_with("http").then(|| target.to_owned())
        })
        .collect()
}
