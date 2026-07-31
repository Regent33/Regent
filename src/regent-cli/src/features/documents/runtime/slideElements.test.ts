import { describe, expect, test } from "bun:test";
import { SHAPE_NAMES, shapeName } from "./slideElements.ts";
import type { SlideElement } from "./types.ts";

const el = (over: Partial<SlideElement>): SlideElement => ({
  kind: "shape",
  x: 1,
  y: 1,
  w: 1,
  h: 1,
  ...over,
});

describe("shapeName", () => {
  // The gap this closes: the vocabulary was rectangle and rounded rectangle, so
  // a deck could not ask for a circle, an arrow or a callout even though the
  // renderer draws them all.
  test("plain-English names resolve to PptxGenJS shapes", () => {
    expect(shapeName(el({ shape: "circle" }))).toBe("ellipse");
    expect(shapeName(el({ shape: "arrow" }))).toBe("rightArrow");
    expect(shapeName(el({ shape: "star" }))).toBe("star5");
    expect(shapeName(el({ shape: "bubble" }))).toBe("wedgeEllipseCallout");
    expect(shapeName(el({ shape: "line" }))).toBe("line");
  });

  test("spacing and case are forgiven", () => {
    for (const written of ["roundRect", "round rect", "round-rect", "ROUNDRECT"]) {
      expect(shapeName(el({ shape: written }))).toBe("roundRect");
    }
  });

  // A hallucinated name written straight into the XML gives a file PowerPoint
  // refuses to open, so unknown values fall back rather than pass through.
  test("an unknown shape falls back to a rectangle", () => {
    expect(shapeName(el({ shape: "dodecahedron" }))).toBe("rect");
    expect(shapeName(el({ shape: "" }))).toBe("rect");
    expect(shapeName(el({}))).toBe("rect");
  });

  test("the old `rounded` flag still works when no shape is named", () => {
    expect(shapeName(el({ rounded: true }))).toBe("roundRect");
    // An explicit shape wins over the legacy flag.
    expect(shapeName(el({ rounded: true, shape: "circle" }))).toBe("ellipse");
  });

  test("every advertised name maps to something", () => {
    expect(SHAPE_NAMES.length).toBeGreaterThan(20);
    for (const name of SHAPE_NAMES) {
      expect(shapeName(el({ shape: name }))).not.toBe("");
    }
  });
});
