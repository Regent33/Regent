// Editable PowerPoint via PptxGenJS. Each layout is a distinct visual recipe and
// the theme (palette + fonts) varies per deck, so two decks no longer come out
// looking like the same template with the words swapped.

import { type Result, err, failure, ok } from "@shared/kernel/result.ts";
import pptxgen from "pptxgenjs";
import type { ChartSpec, DeckSlide, DeckSpec, DeckTheme } from "./types.ts";

const SLIDE_W = 13.333; // 16:9 inches
const SLIDE_H = 7.5;

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

type Pptx = InstanceType<typeof pptxgen>;
type Slide = ReturnType<Pptx["addSlide"]>;

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

// Enumerated bullets as a grid of rounded cards, each with an accent number
// badge — the "agenda / overview" look a plain bullet list can't reach. A lone
// final card spans full width so the grid never leaves a ragged gap.
// ponytail: two columns, sized to the count; keep grid slides to ~4-6 items
// (steered in SKILL.md) or the cards get cramped — that's the model's call.
function gridLayout(pptx: Pptx, s: Slide, theme: DeckTheme, slide: DeckSlide) {
  heading(pptx, s, theme, slide);
  const items = slide.bullets ?? [];
  if (items.length === 0) return;
  const cols = 2;
  const gap = 0.4;
  const x0 = 0.85;
  const y0 = 2.2;
  const gridW = 11.6;
  const cardW = (gridW - gap * (cols - 1)) / cols;
  const rows = Math.ceil(items.length / cols);
  const cardH = Math.max(0.9, (SLIDE_H - y0 - 0.75 - gap * (rows - 1)) / rows);

  items.forEach((text, i) => {
    const col = i % cols;
    const row = Math.floor(i / cols);
    const lastAlone = i === items.length - 1 && items.length % cols === 1;
    const x = lastAlone ? x0 : x0 + col * (cardW + gap);
    const w = lastAlone ? gridW : cardW;
    const y = y0 + row * (cardH + gap);

    s.addShape(pptx.ShapeType.roundRect, {
      x,
      y,
      w,
      h: cardH,
      rectRadius: 0.08,
      fill: { color: theme.accent, transparency: 90 },
      line: { color: theme.accent, width: 1 },
    });
    const badge = 0.55;
    const bx = x + 0.3;
    const by = y + (cardH - badge) / 2;
    s.addShape(pptx.ShapeType.ellipse, {
      x: bx,
      y: by,
      w: badge,
      h: badge,
      fill: { color: theme.accent },
    });
    s.addText(String(i + 1), {
      x: bx,
      y: by,
      w: badge,
      h: badge,
      fontSize: 16,
      bold: true,
      color: theme.coverText,
      align: "center",
      valign: "middle",
      fontFace: theme.titleFont,
    });
    s.addText(text, {
      x: bx + badge + 0.25,
      y: y + 0.1,
      w: w - badge - 0.7,
      h: cardH - 0.2,
      fontSize: 15,
      color: theme.text,
      valign: "middle",
      fontFace: theme.bodyFont,
    });
  });
}

function heading(pptx: Pptx, s: Slide, theme: DeckTheme, slide: DeckSlide) {
  s.addText(slide.title ?? "", {
    x: 0.85,
    y: 0.6,
    w: 11.6,
    h: 0.9,
    fontSize: 28,
    color: theme.text,
    bold: true,
    fontFace: theme.titleFont,
  });
  s.addShape(pptx.ShapeType.rect, {
    x: 0.9,
    y: 1.55,
    w: 2.0,
    h: 0.05,
    fill: { color: theme.accent },
  });
  if (slide.subtitle) {
    s.addText(slide.subtitle, {
      x: 0.85,
      y: 1.65,
      w: 11.6,
      h: 0.5,
      fontSize: 15,
      color: theme.muted,
      fontFace: theme.bodyFont,
    });
  }
}

function addBullets(
  s: Slide,
  theme: DeckTheme,
  bullets: readonly string[] | undefined,
  color: string,
  x: number,
  y: number,
  w: number,
  h: number,
) {
  if (!bullets || bullets.length === 0) return;
  s.addText(
    bullets.map((text) => ({
      text,
      options: {
        bullet: { code: "2022", indent: 18 },
        color,
        fontSize: 16,
        fontFace: theme.bodyFont,
        paraSpaceAfter: 10,
      },
    })),
    { x, y, w, h, valign: "top" },
  );
}

function addChart(
  pptx: Pptx,
  s: Slide,
  chart: ChartSpec,
  x: number,
  y: number,
  w: number,
  h: number,
) {
  const type =
    chart.kind === "line"
      ? pptx.ChartType.line
      : chart.kind === "pie"
        ? pptx.ChartType.pie
        : pptx.ChartType.bar;
  const data = chart.series.map((series) => ({
    name: series.name,
    labels: [...series.labels],
    values: [...series.values],
  }));
  s.addChart(type, data, {
    x,
    y,
    w,
    h,
    showLegend: chart.series.length > 1,
    showTitle: false,
    showValue: chart.kind !== "line",
  });
}

function addImage(s: Slide, base64: string, x: number, y: number, w: number, h: number) {
  s.addImage({
    data: `data:image/png;base64,${base64}`,
    x,
    y,
    w,
    h,
    sizing: { type: "cover", w, h },
  });
}
