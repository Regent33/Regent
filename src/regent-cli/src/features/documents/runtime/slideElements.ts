// Model-placed elements — the deck's design escape hatch, where the model
// composes a slide itself instead of picking one of seven recipes.
//
// Split out of presentation.ts when shapes stopped being "rectangle or rounded
// rectangle": that was the whole vocabulary, so a model that wanted a circle, an
// arrow or a callout could not ask for one, even though PptxGenJS draws them all
// and Regent's own grid layout uses ellipses internally.

import type pptxgen from "pptxgenjs";
import type { Pptx, Slide } from "./slideBlocks.ts";
import type { DeckTheme, SlideElement } from "./types.ts";

/** The shapes a deck may ask for, mapped to PptxGenJS names.
 *
 * A whitelist rather than a pass-through: PptxGenJS accepts any string and
 * writes it straight into the XML, so a hallucinated shape name would produce a
 * file PowerPoint refuses to open. Plain-English aliases sit alongside the
 * official names because a model reaches for "circle" long before "ellipse". */
const SHAPES: Record<string, string> = {
  // Rectangles
  rect: "rect",
  rectangle: "rect",
  square: "rect",
  roundrect: "roundRect",
  rounded: "roundRect",
  // Round
  ellipse: "ellipse",
  circle: "ellipse",
  oval: "ellipse",
  donut: "donut",
  pie: "pie",
  arc: "arc",
  // Angular
  triangle: "triangle",
  righttriangle: "rtTriangle",
  diamond: "diamond",
  parallelogram: "parallelogram",
  trapezoid: "trapezoid",
  pentagon: "pentagon",
  hexagon: "hexagon",
  octagon: "octagon",
  // Emphasis
  star: "star5",
  star5: "star5",
  star6: "star6",
  heart: "heart",
  cloud: "cloud",
  plus: "mathPlus",
  // Direction
  line: "line",
  arrow: "rightArrow",
  rightarrow: "rightArrow",
  leftarrow: "leftArrow",
  uparrow: "upArrow",
  downarrow: "downArrow",
  chevron: "chevron",
  // Annotation
  callout: "wedgeRectCallout",
  bubble: "wedgeEllipseCallout",
  speechbubble: "wedgeEllipseCallout",
};

/** The PptxGenJS shape for an element.
 *
 * `shape` wins; `rounded: true` is still honoured for decks written before
 * shapes had names. An unknown name falls back to a rectangle rather than
 * throwing — one mislabelled shape must not cost the whole deck. */
export function shapeName(element: SlideElement): string {
  const named = element.shape
    ?.trim()
    .toLowerCase()
    .replace(/[\s_-]/g, "");
  if (named !== undefined && named !== "" && SHAPES[named] !== undefined) return SHAPES[named];
  return element.rounded === true ? "roundRect" : "rect";
}

/** Draw model-placed elements. Geometry is trusted as given (it's the point of
 * the escape hatch); only the theme supplies defaults, so an element that omits
 * a colour still matches the deck. Unknown kinds are skipped rather than
 * throwing — one bad element must not lose the whole deck. */
export function renderElements(
  _pptx: Pptx,
  s: Slide,
  theme: DeckTheme,
  elements: readonly SlideElement[],
) {
  for (const el of elements) {
    const box = { x: el.x, y: el.y, w: el.w, h: el.h };
    // Rotation and transparency apply to every kind PowerPoint allows them on.
    const common = {
      ...(el.rotate === undefined ? {} : { rotate: el.rotate }),
    };
    if (el.kind === "text") {
      s.addText(el.text ?? "", {
        ...box,
        ...common,
        fontSize: el.fontSize ?? 14,
        fontFace: el.fontFace ?? theme.bodyFont,
        color: el.color ?? theme.text,
        bold: el.bold ?? false,
        italic: el.italic ?? false,
        align: el.align ?? "left",
        valign: el.valign ?? "top",
      });
    } else if (el.kind === "shape") {
      s.addShape(shapeName(el) as Parameters<Slide["addShape"]>[0], {
        ...box,
        ...common,
        // A line has no interior; filling one draws a bar instead.
        ...(shapeName(el) === "line"
          ? {}
          : {
              fill: {
                color: el.fill ?? theme.accent,
                ...(el.transparency === undefined ? {} : { transparency: el.transparency }),
              },
            }),
        ...(el.line === undefined && shapeName(el) !== "line"
          ? {}
          : {
              line: {
                color: el.line ?? theme.accent,
                width: el.lineWidth ?? 1,
                ...(el.dashed === true ? { dashType: "dash" as const } : {}),
              },
            }),
      });
    } else if (el.kind === "image" && el.imageBase64 !== undefined) {
      s.addImage({ ...box, ...common, data: `image/png;base64,${el.imageBase64}` });
    }
  }
}

/** Every shape name a deck may use, for the tool schema and for tests. */
export const SHAPE_NAMES: readonly string[] = Object.keys(SHAPES);

// `pptxgen` is imported for its type only; referencing it keeps the import
// meaningful to the linter.
export type ShapeCapablePptx = InstanceType<typeof pptxgen>;
