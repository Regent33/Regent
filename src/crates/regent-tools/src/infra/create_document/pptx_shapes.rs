//! DrawingML shape primitives for the designed native deck — rectangles/ellipses,
//! text boxes, bullet stacks, and cropped pictures. `pptx_slide` composes these
//! into a slide; the XML-escaping and palette live in `pptx_xml`.

use super::model::{EmbeddedSlideImage, Slide};
use super::pptx_xml::{TEAL, esc};

pub fn bullet_stack(
    slide: &Slide,
    first_id: usize,
    x: i64,
    y: i64,
    width: i64,
    available_height: i64,
    color: &str,
) -> String {
    if slide.bullets.is_empty() {
        return String::new();
    }
    let count = i64::try_from(slide.bullets.len()).unwrap_or(1).max(1);
    let step = (available_height / count).min(650_000);
    let font_size = if count <= 6 { 1_800 } else { 1_600 };
    let mut out = String::new();
    for (index, bullet) in slide.bullets.iter().enumerate() {
        let top = y + i64::try_from(index).unwrap_or(0) * step;
        let marker_id = first_id + index * 2;
        out.push_str(&rect_shape(
            marker_id,
            "Bullet marker",
            x,
            top + 155_000,
            105_000,
            105_000,
            TEAL,
            "ellipse",
        ));
        out.push_str(&text_box(
            marker_id + 1,
            "Bullet",
            x + 260_000,
            top,
            width - 260_000,
            step.max(430_000),
            bullet,
            font_size,
            color,
            false,
        ));
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub fn rect_shape(
    id: usize,
    name: &str,
    x: i64,
    y: i64,
    width: i64,
    height: i64,
    color: &str,
    geometry: &str,
) -> String {
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{name}"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{width}" cy="{height}"/></a:xfrm><a:prstGeom prst="{geometry}"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="{color}"/></a:solidFill><a:ln><a:noFill/></a:ln></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/><a:p/></p:txBody></p:sp>"#,
        name = esc(name)
    )
}

#[allow(clippy::too_many_arguments)]
pub fn text_box(
    id: usize,
    name: &str,
    x: i64,
    y: i64,
    width: i64,
    height: i64,
    text: &str,
    size: usize,
    color: &str,
    bold: bool,
) -> String {
    let bold = if bold { "1" } else { "0" };
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{name}"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{width}" cy="{height}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/><a:ln><a:noFill/></a:ln></p:spPr><p:txBody><a:bodyPr wrap="square" anchor="t" lIns="0" tIns="0" rIns="0" bIns="0"/><a:lstStyle/><a:p><a:pPr algn="l"/><a:r><a:rPr lang="en-US" sz="{size}" b="{bold}"><a:solidFill><a:srgbClr val="{color}"/></a:solidFill><a:latin typeface="Aptos"/></a:rPr><a:t>{text}</a:t></a:r><a:endParaRPr lang="en-US"/></a:p></p:txBody></p:sp>"#,
        name = esc(name),
        text = esc(text)
    )
}

pub fn picture(
    image: &EmbeddedSlideImage,
    id: usize,
    x: i64,
    y: i64,
    width: i64,
    height: i64,
) -> String {
    let crop = crop_rect(image.width, image.height, width, height);
    format!(
        r#"<p:pic><p:nvPicPr><p:cNvPr id="{id}" name="Slide visual" descr="{alt}"/><p:cNvPicPr><a:picLocks noChangeAspect="1"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="rId2"/><a:srcRect {crop}/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{width}" cy="{height}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:ln><a:noFill/></a:ln></p:spPr></p:pic>"#,
        alt = esc(&image.alt_text)
    )
}

fn crop_rect(source_width: u32, source_height: u32, box_width: i64, box_height: i64) -> String {
    let source_ratio = f64::from(source_width) / f64::from(source_height.max(1));
    let box_ratio = box_width as f64 / box_height.max(1) as f64;
    if source_ratio > box_ratio {
        let crop = ((1.0 - box_ratio / source_ratio) * 50_000.0).round() as i64;
        format!("l=\"{crop}\" r=\"{crop}\" t=\"0\" b=\"0\"")
    } else {
        let crop = ((1.0 - source_ratio / box_ratio) * 50_000.0).round() as i64;
        format!("l=\"0\" r=\"0\" t=\"{crop}\" b=\"{crop}\"")
    }
}
