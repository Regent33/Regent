//! Content-dependent OOXML parts for designed 16:9 presentations.

use super::model::{EmbeddedSlideImage, Slide};

const NAVY: &str = "171C2C";
const TEAL: &str = "00A19B";
const WARM: &str = "F6F2EA";
const WHITE: &str = "FFFFFF";
const MUTED: &str = "667085";

/// XML-escapes user text and attribute values.
pub fn esc(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn content_types(slides: &[Slide]) -> String {
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
"#,
    );
    for extension in ["png", "jpg"] {
        if let Some(image) = slides.iter().find_map(|slide| {
            slide
                .embedded_image
                .as_ref()
                .filter(|image| image.extension == extension)
        }) {
            out.push_str(&format!(
                "<Default Extension=\"{extension}\" ContentType=\"{}\"/>\n",
                image.content_type
            ));
        }
    }
    out.push_str(
        r#"<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>
<Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/>
<Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/>
<Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>
"#,
    );
    for index in 1..=slides.len() {
        out.push_str(&format!(
            "<Override PartName=\"/ppt/slides/slide{index}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>\n"
        ));
    }
    if slides.iter().any(|slide| slide.notes.is_some()) {
        out.push_str("<Override PartName=\"/ppt/notesMasters/notesMaster1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.notesMaster+xml\"/>\n");
        for (index, slide) in slides.iter().enumerate() {
            if slide.notes.is_some() {
                out.push_str(&format!(
                    "<Override PartName=\"/ppt/notesSlides/notesSlide{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml\"/>\n",
                    index + 1
                ));
            }
        }
    }
    out.push_str("</Types>");
    out
}

pub fn presentation(slides: &[Slide], has_notes: bool) -> String {
    let ids = (1..=slides.len())
        .map(|index| {
            format!(
                "<p:sldId id=\"{}\" r:id=\"rId{}\"/>",
                255 + index,
                index + 1
            )
        })
        .collect::<String>();
    let notes_list = if has_notes {
        format!(
            "<p:notesMasterIdLst><p:notesMasterId r:id=\"rId{}\"/></p:notesMasterIdLst>",
            slides.len() + 2
        )
    } else {
        String::new()
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>
<p:sldIdLst>{ids}</p:sldIdLst>{notes_list}
<p:sldSz cx="12192000" cy="6858000" type="screen16x9"/><p:notesSz cx="6858000" cy="9144000"/>
</p:presentation>"#
    )
}

pub fn presentation_rels(slides: &[Slide], has_notes: bool) -> String {
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="slideMasters/slideMaster1.xml"/>
"#,
    );
    for index in 1..=slides.len() {
        out.push_str(&format!(
            "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide\" Target=\"slides/slide{index}.xml\"/>\n",
            index + 1
        ));
    }
    if has_notes {
        out.push_str(&format!(
            "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesMaster\" Target=\"notesMasters/notesMaster1.xml\"/>\n",
            slides.len() + 2
        ));
    }
    out.push_str("</Relationships>");
    out
}

/// A designed slide: minimal cover, then alternating editorial layouts. A
/// supplied image gets a cropped split layout; text-only slides retain strong
/// hierarchy and ample whitespace rather than falling back to a default theme.
pub fn slide(slide: &Slide, number: usize, total: usize) -> String {
    let is_cover = number == 1;
    let background = if is_cover {
        NAVY
    } else if number.is_multiple_of(2) {
        WARM
    } else {
        WHITE
    };
    let muted = if is_cover { "CBD5E1" } else { MUTED };
    let has_image = slide.embedded_image.is_some();

    let mut shapes = String::new();
    if is_cover {
        shapes.push_str(&rect_shape(
            2,
            "Accent rail",
            0,
            0,
            145_000,
            6_858_000,
            TEAL,
            "rect",
        ));
        shapes.push_str(&rect_shape(
            3,
            "Kicker",
            820_000,
            620_000,
            2_340_000,
            350_000,
            TEAL,
            "roundRect",
        ));
        shapes.push_str(&text_box(
            4,
            "Kicker text",
            970_000,
            675_000,
            2_050_000,
            220_000,
            "REGENT  /  PRESENTATION",
            1_100,
            WHITE,
            true,
        ));
        let title_width = if has_image { 5_650_000 } else { 10_300_000 };
        shapes.push_str(&text_box(
            5,
            "Title",
            830_000,
            1_330_000,
            title_width,
            1_750_000,
            &slide.title,
            5_000,
            WHITE,
            true,
        ));
        if let Some(subtitle) = &slide.subtitle {
            shapes.push_str(&text_box(
                6,
                "Subtitle",
                840_000,
                3_200_000,
                title_width,
                750_000,
                subtitle,
                2_400,
                "D9E1EA",
                false,
            ));
        }
        let body_width = if has_image { 5_550_000 } else { 9_900_000 };
        shapes.push_str(&bullet_stack(
            slide, 20, 850_000, 4_190_000, body_width, 1_650_000, WHITE,
        ));
        shapes.push_str(&rect_shape(
            7,
            "Footer accent",
            830_000,
            6_290_000,
            1_100_000,
            70_000,
            TEAL,
            "rect",
        ));
    } else {
        shapes.push_str(&rect_shape(
            2,
            "Section rail",
            730_000,
            560_000,
            115_000,
            720_000,
            TEAL,
            "rect",
        ));
        shapes.push_str(&text_box(
            3,
            "Section label",
            1_030_000,
            565_000,
            2_000_000,
            260_000,
            &format!("INSIGHT  {:02}", number),
            1_150,
            TEAL,
            true,
        ));
        shapes.push_str(&text_box(
            4,
            "Title",
            1_020_000,
            930_000,
            10_250_000,
            900_000,
            &slide.title,
            3_500,
            NAVY,
            true,
        ));
        if let Some(subtitle) = &slide.subtitle {
            shapes.push_str(&text_box(
                5, "Subtitle", 1_030_000, 1_790_000, 9_900_000, 530_000, subtitle, 1_850, MUTED,
                false,
            ));
        }
        shapes.push_str(&rect_shape(
            6, "Rule", 1_030_000, 2_330_000, 10_200_000, 32_000, "D0D5DD", "rect",
        ));
        let body_width = if has_image { 5_450_000 } else { 9_600_000 };
        shapes.push_str(&bullet_stack(
            slide, 20, 1_040_000, 2_690_000, body_width, 3_200_000, NAVY,
        ));
        if has_image {
            shapes.push_str(&rect_shape(
                7,
                "Image offset",
                7_010_000,
                2_560_000,
                4_300_000,
                3_500_000,
                TEAL,
                "roundRect",
            ));
        } else {
            shapes.push_str(&text_box(
                7,
                "Watermark",
                9_850_000,
                5_100_000,
                1_300_000,
                900_000,
                &format!("{:02}", number),
                5_600,
                "D7DDD9",
                true,
            ));
        }
    }
    shapes.push_str(&text_box(
        90,
        "Slide count",
        10_650_000,
        6_330_000,
        700_000,
        220_000,
        &format!("{:02} / {:02}", number, total),
        1_050,
        muted,
        true,
    ));
    if let Some(image) = &slide.embedded_image {
        let (x, y, width, height) = if is_cover {
            (7_250_000, 0, 4_942_000, 6_858_000)
        } else {
            (7_140_000, 2_440_000, 4_300_000, 3_500_000)
        };
        shapes.push_str(&picture(image, 80, x, y, width, height));
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:cSld><p:bg><p:bgPr><a:solidFill><a:srgbClr val="{background}"/></a:solidFill><a:effectLst/></p:bgPr></p:bg><p:spTree>
<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>
{shapes}
</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"#
    )
}

fn bullet_stack(
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
fn rect_shape(
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
fn text_box(
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

fn picture(
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

pub fn notes_slide(notes: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:notes xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/><p:sp><p:nvSpPr><p:cNvPr id="2" name="Notes Placeholder 1"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>{}</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:notes>"#,
        esc(notes)
    )
}

pub fn slide_rels(number: usize, has_notes: bool, image: Option<&EmbeddedSlideImage>) -> String {
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
"#,
    );
    if let Some(image) = image {
        out.push_str(&format!(
            "<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"../media/image{number}.{}\"/>\n",
            image.extension
        ));
    }
    if has_notes {
        let id = if image.is_some() { 3 } else { 2 };
        out.push_str(&format!(
            "<Relationship Id=\"rId{id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide\" Target=\"../notesSlides/notesSlide{number}.xml\"/>\n"
        ));
    }
    out.push_str("</Relationships>");
    out
}

pub fn notes_slide_rels(number: usize) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="../slides/slide{number}.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesMaster" Target="../notesMasters/notesMaster1.xml"/>
</Relationships>"#
    )
}
