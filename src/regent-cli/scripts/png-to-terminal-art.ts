// Dev tool — rasterize a PNG into terminal-art cells and emit a TS data module
// (so the runtime/binary needs no image decoder, just the data).
//
//   bun run scripts/png-to-terminal-art.ts <png> <out.ts> <EXPORT_NAME> [cols] [mode]
//
// mode = "half" (default): each cell is a ▀ with fg = top source pixel, bg =
//   bottom one (2 px/row), colour alpha-weighted from the source.
// mode = "braille": each cell is a 2×4 braille glyph (Hermes CADUCEUS style),
//   coloured by a top→bottom TEAL gradient (the brand wordmark gradient) rather
//   than the source colours — a crisp monochrome-teal silhouette.
import { readFileSync, writeFileSync } from "node:fs";
import { PNG } from "pngjs";

const [, , input, outPath, exportName, colsArg, modeArg] = process.argv;
if (!input || !outPath || !exportName) {
  console.error(
    "usage: bun run scripts/png-to-terminal-art.ts <png> <out.ts> <EXPORT_NAME> [cols] [half|braille]",
  );
  process.exit(1);
}
const mode = modeArg === "braille" ? "braille" : "half";
const targetCols = Number(colsArg ?? (mode === "braille" ? 22 : 26));

const png = PNG.sync.read(readFileSync(input));
const { width, height, data } = png;
const px = (x: number, y: number): [number, number, number, number] => {
  const i = (y * width + x) * 4;
  return [data[i] ?? 0, data[i + 1] ?? 0, data[i + 2] ?? 0, data[i + 3] ?? 0];
};

// Bounding box of non-transparent pixels (trim the transparent margin).
let minX = width;
let minY = height;
let maxX = 0;
let maxY = 0;
for (let y = 0; y < height; y++) {
  for (let x = 0; x < width; x++) {
    if (px(x, y)[3] > 16) {
      if (x < minX) minX = x;
      if (x > maxX) maxX = x;
      if (y < minY) minY = y;
      if (y > maxY) maxY = y;
    }
  }
}
const bw = maxX - minX + 1;
const bh = maxY - minY + 1;

// Box-average the source region mapping to output pixel (ox, oy) of an outW×outH
// grid; colour is alpha-weighted, alpha is the plain mean (edge cells fade out).
const sampleAt = (
  ox: number,
  oy: number,
  outW: number,
  outH: number,
): [number, number, number, number] | null => {
  const sx0 = minX + Math.floor((ox * bw) / outW);
  const sx1 = Math.max(minX + Math.floor(((ox + 1) * bw) / outW), sx0 + 1);
  const sy0 = minY + Math.floor((oy * bh) / outH);
  const sy1 = Math.max(minY + Math.floor(((oy + 1) * bh) / outH), sy0 + 1);
  let r = 0;
  let g = 0;
  let b = 0;
  let aw = 0;
  let asum = 0;
  let n = 0;
  for (let y = sy0; y < sy1 && y < height; y++) {
    for (let x = sx0; x < sx1 && x < width; x++) {
      const [pr, pg, pb, pa] = px(x, y);
      r += pr * pa;
      g += pg * pa;
      b += pb * pa;
      aw += pa;
      asum += pa;
      n++;
    }
  }
  if (n === 0 || aw === 0) return null;
  return [Math.round(r / aw), Math.round(g / aw), Math.round(b / aw), Math.round(asum / n)];
};

const hex = (c: [number, number, number, number]): string =>
  `#${c[0].toString(16).padStart(2, "0")}${c[1].toString(16).padStart(2, "0")}${c[2]
    .toString(16)
    .padStart(2, "0")}`;

// Teal gradient (matches theme.ts tealRamp): light → brand → deep.
const tealRamp = ["#5FD3CD", "#19B3AC", "#00A19B", "#0B8782"];
const tealShade = (i: number, n: number): string =>
  n <= 1
    ? (tealRamp[0] as string)
    : (tealRamp[Math.min(Math.floor((i * (tealRamp.length - 1)) / (n - 1)), tealRamp.length - 1)] ??
      tealRamp[0]) as string;

const VISIBLE = 80; // alpha threshold below which a sub-pixel is "transparent"
// Braille dots use a lower threshold than the half-block path so shaded pixels
// still fill in — a denser, more solid silhouette. Tunable via the 7th arg.
const BRAILLE_DOT = Number(process.argv[7] ?? 55);
type Cell = { char: string; color?: string; bg?: string };
const rows: Cell[][] = [];

if (mode === "braille") {
  // Each cell = a 2-wide × 4-tall block of braille dots.
  const dotW = targetCols * 2;
  const dotH = Math.max(4, Math.round((dotW * bh) / bw / 4) * 4);
  const cellRows = dotH / 4;
  // Dot bit per (dx, dy) in the 2×4 block (Unicode braille ordering).
  const DOT_BIT = [
    [0x01, 0x02, 0x04, 0x40],
    [0x08, 0x10, 0x20, 0x80],
  ];
  for (let cr = 0; cr < cellRows; cr++) {
    const cells: Cell[] = [];
    for (let cc = 0; cc < targetCols; cc++) {
      let bits = 0;
      for (let dx = 0; dx < 2; dx++) {
        for (let dy = 0; dy < 4; dy++) {
          const s = sampleAt(cc * 2 + dx, cr * 4 + dy, dotW, dotH);
          if (s && s[3] >= BRAILLE_DOT) bits |= DOT_BIT[dx]?.[dy] ?? 0;
        }
      }
      if (bits === 0) {
        cells.push({ char: " " });
        continue;
      }
      cells.push({ char: String.fromCodePoint(0x2800 + bits), color: tealShade(cr, cellRows) });
    }
    rows.push(cells);
  }
} else {
  const pxW = targetCols;
  const pxH = Math.max(2, Math.round((pxW * bh) / bw));
  for (let cy = 0; cy < pxH; cy += 2) {
    const cells: Cell[] = [];
    for (let cx = 0; cx < pxW; cx++) {
      const top = sampleAt(cx, cy, pxW, pxH);
      const bot = cy + 1 < pxH ? sampleAt(cx, cy + 1, pxW, pxH) : null;
      const topVis = top !== null && top[3] >= VISIBLE;
      const botVis = bot !== null && bot[3] >= VISIBLE;
      if (topVis && top && botVis && bot) cells.push({ char: "▀", color: hex(top), bg: hex(bot) });
      else if (topVis && top) cells.push({ char: "▀", color: hex(top) });
      else if (botVis && bot) cells.push({ char: "▄", color: hex(bot) });
      else cells.push({ char: " " });
    }
    rows.push(cells);
  }
}

writeFileSync(
  outPath,
  `// GENERATED by scripts/png-to-terminal-art.ts from ${input} (${mode}) — do not edit by hand.\n` +
    `import type { ArtCell } from "@shared/ui/brand/art.ts";\n\n` +
    `export const ${exportName}: ArtCell[][] = ${JSON.stringify(rows)};\n`,
);
console.log(`wrote ${outPath}: ${rows.length} rows × ${targetCols} cols (${mode}, from ${bw}x${bh} bbox)`);
