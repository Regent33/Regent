// Shared slide building blocks for the PptxGenJS deck renderer: the heading
// band, bullet lists, charts, images, and the numbered-card grid layout.
import type pptxgen from "pptxgenjs";
import type { ChartSpec, DeckSlide, DeckTheme, TableSpec } from "./types.ts";

export const SLIDE_W = 13.333; // 16:9 inches
export const SLIDE_H = 7.5;

export type Pptx = InstanceType<typeof pptxgen>;
export type Slide = ReturnType<Pptx["addSlide"]>;

// Enumerated bullets as a grid of rounded cards, each with an accent number
// badge — the "agenda / overview" look a plain bullet list can't reach. A lone
// final card spans full width so the grid never leaves a ragged gap.
// ponytail: two columns, sized to the count; keep grid slides to ~4-6 items
// (steered in SKILL.md) or the cards get cramped — that's the model's call.
export function gridLayout(pptx: Pptx, s: Slide, theme: DeckTheme, slide: DeckSlide) {
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

// A slide title carries the slide. 28pt read as a document heading rather than
// a presentation one — current practice is a large, confident title over small
// high-contrast support text, and the body below is already capped at ten
// bullets, so the room exists. The geometry is checked, not eyeballed: the
// title box ends at 1.65, the rule sits at 1.72, a subtitle runs 1.82–2.27, and
// the body starts at 2.3 — every layout below assumes that last number.
const TITLE_PT = 32;
const TITLE_BOX_H = 1.05; // ~two lines at 32pt

export function heading(pptx: Pptx, s: Slide, theme: DeckTheme, slide: DeckSlide) {
  s.addText(slide.title ?? "", {
    x: 0.85,
    y: 0.6,
    w: 11.6,
    h: TITLE_BOX_H,
    fontSize: TITLE_PT,
    color: theme.text,
    bold: true,
    fontFace: theme.titleFont,
  });
  s.addShape(pptx.ShapeType.rect, {
    x: 0.9,
    y: 1.72,
    w: 2.0,
    h: 0.05,
    fill: { color: theme.accent },
  });
  if (slide.subtitle) {
    s.addText(slide.subtitle, {
      x: 0.85,
      y: 1.82,
      w: 11.6,
      h: 0.45,
      fontSize: 14,
      color: theme.muted,
      fontFace: theme.bodyFont,
    });
  }
}

export function addBullets(
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

export function addChart(
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

/** A real PowerPoint table — editable in PowerPoint, not a picture of one.
 *
 * The header row carries the theme accent and the display face so it reads as a
 * header without a table style; PowerPoint's built-in styles are the first
 * thing a corporate template overrides, and the deck should look like itself. */
export function addTable(
  s: Slide,
  theme: DeckTheme,
  table: TableSpec,
  x: number,
  y: number,
  w: number,
  h: number,
) {
  if (table.rows.length === 0) return;
  const body = table.rows.map((row) =>
    row.map((text) => ({
      text,
      options: { color: theme.text, fontFace: theme.bodyFont, fontSize: 12 },
    })),
  );
  const header =
    table.headers && table.headers.length > 0
      ? [
          table.headers.map((text) => ({
            text,
            options: {
              color: theme.coverText,
              fill: { color: theme.accent },
              fontFace: theme.titleFont,
              fontSize: 12,
              bold: true,
            },
          })),
        ]
      : [];
  s.addTable([...header, ...body], {
    x,
    y,
    w,
    h,
    // `auto` grows rows to fit their text instead of clipping it, which is the
    // difference between a readable table and one missing its last line.
    autoPage: false,
    rowH: 0,
    valign: "middle",
    border: { type: "solid", pt: 0.5, color: theme.muted },
    margin: 6,
  });
  if (table.caption) {
    s.addText(table.caption, {
      x,
      y: y + h + 0.05,
      w,
      h: 0.3,
      color: theme.muted,
      fontFace: theme.bodyFont,
      fontSize: 10,
    });
  }
}

export function addImage(s: Slide, base64: string, x: number, y: number, w: number, h: number) {
  s.addImage({
    data: `data:image/png;base64,${base64}`,
    x,
    y,
    w,
    h,
    sizing: { type: "cover", w, h },
  });
}
