// Regent's "REGENT" wordmark — a bold, outlined pixel font (the HERMES-AGENT
// display look) rendered as half-block art cells. The kneeling-king mark comes
// from the real logo PNG (see kingArt.generated.ts), not from here.
import { tealShade } from "@shared/ui/tokens/theme.ts";

/** A rendered half-block cell: `color` is the top sub-pixel, `bg` the bottom. */
export interface ArtCell {
  readonly char: string;
  readonly color?: string;
  readonly bg?: string;
}

// 5×7 pixel glyphs for the six letters of REGENT.
const GLYPHS: Record<string, readonly string[]> = {
  R: ["####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#"],
  E: ["#####", "#....", "#....", "####.", "#....", "#....", "#####"],
  G: [".####", "#....", "#....", "#.###", "#...#", "#...#", ".####"],
  N: ["#...#", "##..#", "#.#.#", "#.#.#", "#..##", "#...#", "#...#"],
  T: ["#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#.."],
};

// 3D bevel palette: a bright top-left rim (light hitting a raised block) and a
// dark bottom-right extrusion (the block's depth), with the gradient face
// between — the chunky, extruded HERMES-AGENT display look, in teal.
const RIM = "#B8F7F1"; // top-left highlight
const EDGE = "#063F3B"; // bottom-right extruded side (deep shade)
const DEPTH = 2; // how far the block is "raised" off the background

/**
 * The "REGENT" wordmark as chunky, 3D-extruded half-block cells: a teal
 * gradient face, a bright rim along the top-left, and a dark extruded side
 * falling away to the bottom-right — so each letter reads as a raised solid
 * block rather than a flat outline.
 */
export function renderWordmark(): ArtCell[][] {
  const scale = 2;
  const glyphW = 5 * scale;
  const glyphH = 7 * scale;
  const gap = 3;
  const pad = 1;
  const word = [..."REGENT"];
  // Extra room on the right/bottom for the extruded side.
  const width = word.length * glyphW + (word.length - 1) * gap + pad * 2 + DEPTH;
  const height = glyphH + pad * 2 + DEPTH;

  // Stamp the glyph face.
  const fill: boolean[][] = Array.from({ length: height }, () =>
    new Array<boolean>(width).fill(false),
  );
  let x = pad;
  for (const letter of word) {
    const glyph = GLYPHS[letter];
    if (glyph) {
      glyph.forEach((row, gy) => {
        [...row].forEach((ch, gx) => {
          if (ch !== "#") return;
          for (let dy = 0; dy < scale; dy++)
            for (let dx = 0; dx < scale; dx++) {
              const r = fill[pad + gy * scale + dy];
              if (r) r[x + gx * scale + dx] = true;
            }
        });
      });
    }
    x += glyphW + gap;
  }

  // value grid: 3 = face, 2 = extruded side (down-right of a face pixel),
  // 1 = bright rim (empty pixel bordering the face on its top/left), 0 = empty.
  const val: number[][] = Array.from({ length: height }, () => new Array<number>(width).fill(0));
  for (let y = 0; y < height; y++) {
    const vrow = val[y];
    if (!vrow) continue;
    for (let xx = 0; xx < width; xx++) {
      if (fill[y]?.[xx]) {
        vrow[xx] = 3;
        continue;
      }
      // Extruded side: this pixel sits diagonally behind a face pixel.
      let extruded = false;
      for (let d = 1; d <= DEPTH; d++) {
        if (fill[y - d]?.[xx - d]) {
          extruded = true;
          break;
        }
      }
      if (extruded) {
        vrow[xx] = 2;
        continue;
      }
      // Bright rim: face pixel is directly below / to the right (top-left edge).
      if (fill[y + 1]?.[xx] || fill[y]?.[xx + 1] || fill[y + 1]?.[xx + 1]) vrow[xx] = 1;
    }
  }

  // Colour for a non-empty value: gradient face, dark extruded side, bright rim.
  const colorFor = (v: number, y: number): string =>
    v === 3 ? tealShade(y - pad, glyphH) : v === 2 ? EDGE : RIM;

  const rows: ArtCell[][] = [];
  for (let cy = 0; cy < height; cy += 2) {
    const cells: ArtCell[] = [];
    for (let xx = 0; xx < width; xx++) {
      const top = val[cy]?.[xx] ?? 0;
      const bot = val[cy + 1]?.[xx] ?? 0;
      if (top && bot)
        cells.push({ char: "▀", color: colorFor(top, cy), bg: colorFor(bot, cy + 1) });
      else if (top) cells.push({ char: "▀", color: colorFor(top, cy) });
      else if (bot) cells.push({ char: "▄", color: colorFor(bot, cy + 1) });
      else cells.push({ char: " " });
    }
    rows.push(cells);
  }
  return rows;
}
