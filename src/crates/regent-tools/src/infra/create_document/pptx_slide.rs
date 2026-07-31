//! Composes one designed 16:9 slide from a `Slide` spec: a minimal navy cover,
//! then alternating editorial layouts with strong hierarchy and ample
//! whitespace. Shape primitives come from `pptx_shapes`; the palette and XML
//! escaping from `pptx_xml`.

use super::model::Slide;
use super::pptx_shapes::{bullet_stack, picture, rect_shape, text_box};
use super::pptx_xml::{MUTED, NAVY, TEAL, WARM, WHITE};

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
        // No kicker badge. It read "REGENT / PRESENTATION" on the cover of
        // every deck — the user's work product carrying the name of the tool
        // that typed it. The accent rail already gives the cover its shape.
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
