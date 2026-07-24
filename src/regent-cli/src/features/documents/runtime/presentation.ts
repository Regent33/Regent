// Editable PowerPoint via PptxGenJS. Each layout is a distinct visual recipe and
// the theme (palette + fonts) varies per deck, so two decks no longer come out
// looking like the same template with the words swapped.
// Shared primitives (heading/bullets/chart/image + the card grid) live in
// slideBlocks.ts.

import { type Result, err, failure, ok } from "@shared/kernel/result.ts";
import pptxgen from "pptxgenjs";
import {
  type Pptx,
  SLIDE_H,
  SLIDE_W,
  type Slide,
  addBullets,
  addChart,
  addImage,
  gridLayout,
  heading,
} from "./slideBlocks.ts";
import type { DeckSlide, DeckSpec, DeckTheme } from "./types.ts";

export async function buildPptx(deck: DeckSpec): Promise<Result<Uint8Array>> {
  try {
    const pptx = new pptxgen();
    pptx.defineLayout({ name: "REGENT_16x9", width: SLIDE_W, height: SLIDE_H });
    pptx.layout = "REGENT_16x9";
    const total = deck.slides.length;
    deck.slides.forEach((slide, i) => renderSlide(pptx, deck.theme, slide, i + 1, total));
    const buffer = (await pptx.write({ outputType: "nodebuffer" })) as Uint8Array;
    return ok(new Uint8Array(buffer));
  } catch (cause) {
    return err(failure("pptx-build-failed", "PptxGenJS deck build failed", cause));
  }
}

function renderSlide(pptx: Pptx, theme: DeckTheme, slide: DeckSlide, index: number, total: number) {
  const s = pptx.addSlide();
  s.background = { color: theme.background };
  switch (slide.layout) {
    case "cover":
      coverLayout(pptx, s, theme, slide);
      break;
    case "section":
      sectionLayout(s, theme, slide);
      break;
    case "split":
      splitLayout(pptx, s, theme, slide);
      break;
    case "chart":
      chartLayout(pptx, s, theme, slide);
      break;
    case "grid":
      gridLayout(pptx, s, theme, slide);
      break;
    case "content":
      contentLayout(pptx, s, theme, slide);
      break;
    case "blank":
      break;
  }
  if (slide.notes) s.addNotes(slide.notes);
  s.addText(`${index} / ${total}`, {
    x: 11.9,
    y: 7.0,
    w: 1.2,
    h: 0.3,
    fontSize: 9,
    color: theme.muted,
    align: "right",
    fontFace: theme.bodyFont,
  });
}

function coverLayout(pptx: Pptx, s: Slide, theme: DeckTheme, slide: DeckSlide) {
  s.background = { color: theme.coverBackground };
  s.addShape(pptx.ShapeType.rect, {
    x: 0,
    y: 0,
    w: 0.22,
    h: SLIDE_H,
    fill: { color: theme.accent },
  });
  s.addText((slide.subtitle ?? "PRESENTATION").toUpperCase(), {
    x: 0.9,
    y: 0.8,
    w: 10,
    h: 0.4,
    fontSize: 12,
    color: theme.accent,
    charSpacing: 3,
    bold: true,
    fontFace: theme.bodyFont,
  });
  s.addText(slide.title ?? "", {
    x: 0.85,
    y: 1.8,
    w: 11.5,
    h: 2.4,
    fontSize: 48,
    color: theme.coverText,
    bold: true,
    fontFace: theme.titleFont,
    valign: "top",
  });
  addBullets(s, theme, slide.bullets, theme.coverText, 0.9, 4.5, 11, 2.2);
}

function sectionLayout(s: Slide, theme: DeckTheme, slide: DeckSlide) {
  s.background = { color: theme.accent };
  s.addText(slide.title ?? "", {
    x: 0.8,
    y: 2.6,
    w: 11.7,
    h: 2.2,
    fontSize: 40,
    color: theme.coverText,
    bold: true,
    align: "center",
    fontFace: theme.titleFont,
  });
  if (slide.subtitle) {
    s.addText(slide.subtitle, {
      x: 1.5,
      y: 4.7,
      w: 10.3,
      h: 0.8,
      fontSize: 18,
      color: theme.coverText,
      align: "center",
      fontFace: theme.bodyFont,
    });
  }
}

function contentLayout(pptx: Pptx, s: Slide, theme: DeckTheme, slide: DeckSlide) {
  heading(pptx, s, theme, slide);
  const hasVisual = Boolean(slide.imageBase64) || Boolean(slide.chart);
  const bodyW = hasVisual ? 6.4 : 11.6;
  addBullets(s, theme, slide.bullets, theme.text, 0.85, 2.3, bodyW, 4.4);
  if (slide.chart) addChart(pptx, s, slide.chart, 7.4, 2.3, 5.1, 4.2);
  else if (slide.imageBase64) addImage(s, slide.imageBase64, 7.4, 2.3, 5.1, 4.2);
}

function splitLayout(pptx: Pptx, s: Slide, theme: DeckTheme, slide: DeckSlide) {
  heading(pptx, s, theme, slide);
  addBullets(s, theme, slide.bullets, theme.text, 0.85, 2.3, 6.2, 4.4);
  if (slide.imageBase64) addImage(s, slide.imageBase64, 7.2, 2.1, 5.3, 4.8);
  else if (slide.chart) addChart(pptx, s, slide.chart, 7.2, 2.3, 5.3, 4.2);
}

function chartLayout(pptx: Pptx, s: Slide, theme: DeckTheme, slide: DeckSlide) {
  heading(pptx, s, theme, slide);
  if (slide.chart) addChart(pptx, s, slide.chart, 0.85, 2.3, 11.6, 4.4);
}
