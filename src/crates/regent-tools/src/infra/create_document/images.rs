//! Keyless image sourcing for `create_document`. A slide can name a `query`
//! instead of a local `path`; we fetch one matching, commercially-licensed photo
//! from Openverse (no API key, no token dance) and embed it. `url` downloads a
//! direct link instead. Both are best-effort by design — a missing photo is the
//! caller's to soften into a note, never a reason to fail the whole document.
//! The network/cache side lives in images_fetch.rs.

use super::model::{
    DocFormat, DocumentSpec, EmbeddedSlideImage, ImageSource, RenderedImage, Slide,
};
use crate::domain::entities::ToolContext;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use fetch::{cache_path, download, search};
use std::io::Cursor;
use std::path::Path;

#[path = "images_fetch.rs"]
mod fetch;

/// Resolve each slide's image to bytes and embed it. `cache_dir` (the deck's own
/// folder) memoizes url/query fetches so an edit reuses the same picture instead
/// of re-searching. Returns soft notes for images that couldn't be sourced — the
/// deck still ships without them; only a jail violation is fatal.
pub async fn hydrate_slides(
    spec: &mut DocumentSpec,
    ctx: &ToolContext,
    cache_dir: Option<&Path>,
) -> Result<Vec<String>, String> {
    if spec.format != DocFormat::Pptx {
        return Ok(Vec::new());
    }
    let mut notes = Vec::new();
    for index in 0..spec.slides.len() {
        let Some(image_spec) = spec.slides[index].image.clone() else {
            continue;
        };
        let bytes = match resolve_source(&image_spec, ctx, cache_dir).await? {
            SourceOutcome::Bytes(bytes) => bytes,
            SourceOutcome::Missing(reason) => {
                notes.push(format!("slide {}: {reason}", index + 1));
                continue;
            }
        };
        if let Err(reason) = embed_image(
            &mut spec.slides[index],
            bytes,
            image_spec.alt_text.as_deref(),
        ) {
            notes.push(format!("slide {}: {reason}", index + 1));
        }
    }
    Ok(notes)
}

/// Resolve each section's image (PDF reports only) into an inline data URI the
/// HTML template embeds. Same soft-failure contract as `hydrate_slides`; DOCX and
/// the native-PDF fallback don't embed images, so this is a no-op for them.
pub async fn hydrate_sections(
    spec: &mut DocumentSpec,
    ctx: &ToolContext,
    cache_dir: Option<&Path>,
) -> Result<Vec<String>, String> {
    if spec.format != DocFormat::Pdf {
        return Ok(Vec::new());
    }
    let mut notes = Vec::new();
    for index in 0..spec.sections.len() {
        let Some(image_spec) = spec.sections[index].image.clone() else {
            continue;
        };
        let bytes = match resolve_source(&image_spec, ctx, cache_dir).await? {
            SourceOutcome::Bytes(bytes) => bytes,
            SourceOutcome::Missing(reason) => {
                notes.push(format!("section {}: {reason}", index + 1));
                continue;
            }
        };
        match normalize_image(bytes) {
            Ok(normalized) => {
                let alt = image_spec
                    .alt_text
                    .clone()
                    .or_else(|| spec.sections[index].heading.clone())
                    .unwrap_or_default();
                spec.sections[index].image_render = Some(RenderedImage {
                    data_uri: format!(
                        "data:{};base64,{}",
                        normalized.content_type,
                        BASE64.encode(&normalized.bytes)
                    ),
                    alt,
                });
            }
            Err(reason) => notes.push(format!("section {}: {reason}", index + 1)),
        }
    }
    Ok(notes)
}

/// Where a slide image's bytes came from — or why they couldn't be had.
enum SourceOutcome {
    Bytes(Vec<u8>),
    /// Soft failure: skip this one image and note it. Never sinks the document.
    Missing(String),
}

enum Fetch {
    Url(String),
    Query(String),
}

/// Fetch a slide image's bytes from its `path` (local, jailed), `url` (direct
/// download), or `query` (keyless Openverse search), in that order. Only a jail
/// violation returns `Err`; everything else that goes wrong is a soft `Missing`.
async fn resolve_source(
    image: &ImageSource,
    ctx: &ToolContext,
    cache_dir: Option<&Path>,
) -> Result<SourceOutcome, String> {
    if let Some(path) = image
        .path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        // A jail escape is a security boundary — hard error. A plain read miss
        // is soft (the model named a file that isn't there; note it and go on).
        let resolved = ctx.resolve(path).map_err(|error| error.to_string())?;
        return Ok(match tokio::fs::read(&resolved).await {
            Ok(bytes) => SourceOutcome::Bytes(bytes),
            Err(error) => {
                SourceOutcome::Missing(format!("cannot read image {}: {error}", resolved.display()))
            }
        });
    }

    // url / query: best-effort and cached, so edits reuse the same picture.
    let (cache_key, fetch) = if let Some(url) = image
        .url
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty())
    {
        (url.to_owned(), Fetch::Url(url.to_owned()))
    } else if let Some(query) = image
        .query
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
    {
        (format!("q:{query}"), Fetch::Query(query.to_owned()))
    } else {
        return Ok(SourceOutcome::Missing(
            "image needs a `path`, `url`, or `query`".to_owned(),
        ));
    };

    let cache = cache_path(cache_dir, &cache_key).await;
    if let Some(cached) = &cache
        && let Ok(bytes) = tokio::fs::read(cached).await
    {
        return Ok(SourceOutcome::Bytes(bytes));
    }

    let url = match fetch {
        Fetch::Url(url) => url,
        Fetch::Query(query) => match search(&query).await {
            Ok(Some(url)) => url,
            Ok(None) => {
                return Ok(SourceOutcome::Missing(format!(
                    "no image found for {query:?}"
                )));
            }
            Err(error) => return Ok(SourceOutcome::Missing(error)),
        },
    };
    match download(&url).await {
        Ok(bytes) => {
            if let Some(cached) = &cache {
                let _ = tokio::fs::write(cached, &bytes).await;
            }
            Ok(SourceOutcome::Bytes(bytes))
        }
        Err(error) => Ok(SourceOutcome::Missing(error)),
    }
}

/// An image decoded and normalized to a format PowerPoint/HTML can embed.
struct NormalizedImage {
    bytes: Vec<u8>,
    extension: &'static str,
    content_type: &'static str,
    width: u32,
    height: u32,
}

/// Decode `bytes` (any format `image` recognizes) and normalize to PNG/JPEG —
/// PowerPoint only embeds those two, so anything else is transcoded to PNG.
fn normalize_image(bytes: Vec<u8>) -> Result<NormalizedImage, String> {
    let format =
        image::guess_format(&bytes).map_err(|error| format!("not a supported image: {error}"))?;
    let decoded = image::load_from_memory_with_format(&bytes, format)
        .map_err(|error| format!("cannot decode image: {error}"))?;
    let (width, height) = (decoded.width(), decoded.height());
    let (bytes, extension, content_type) = match format {
        image::ImageFormat::Png => (bytes, "png", "image/png"),
        image::ImageFormat::Jpeg => (bytes, "jpg", "image/jpeg"),
        _ => {
            let mut encoded = Cursor::new(Vec::new());
            decoded
                .write_to(&mut encoded, image::ImageFormat::Png)
                .map_err(|error| format!("cannot convert image to PNG: {error}"))?;
            (encoded.into_inner(), "png", "image/png")
        }
    };
    Ok(NormalizedImage {
        bytes,
        extension,
        content_type,
        width,
        height,
    })
}

/// Normalize `bytes` and attach them to the slide as an embedded PPTX image.
fn embed_image(slide: &mut Slide, bytes: Vec<u8>, alt_text: Option<&str>) -> Result<(), String> {
    let normalized = normalize_image(bytes)?;
    slide.embedded_image = Some(EmbeddedSlideImage {
        bytes: normalized.bytes,
        extension: normalized.extension,
        content_type: normalized.content_type,
        width: normalized.width,
        height: normalized.height,
        alt_text: alt_text
            .map(str::to_owned)
            .unwrap_or_else(|| slide.title.clone()),
    });
    Ok(())
}
