// Regent's brand art. The "REGENT" wordmark is the **ANSI Shadow** figlet font
// (full-block `█` fills + `╗║╔═╚╝` box-drawing for the 3D bevel) — the same
// chunky, extruded display look as Hermes' "HERMES-AGENT" banner, rendered in a
// teal vertical gradient (see BrandHeader). The kneeling-king mark is separate,
// rasterised from the real logo PNG (see kingArt.generated.ts).

/** A rendered half-block cell (used by the king art): `color` top, `bg` bottom. */
export interface ArtCell {
  readonly char: string;
  readonly color?: string;
  readonly bg?: string;
}

// "REGENT" in the ANSI Shadow font — 6 rows, each letter a fixed-width block.
// BrandHeader colours each row with a teal shade (bright top → deep bottom).
export const REGENT_BANNER: readonly string[] = [
  "██████╗ ███████╗ ██████╗ ███████╗███╗   ██╗████████╗",
  "██╔══██╗██╔════╝██╔════╝ ██╔════╝████╗  ██║╚══██╔══╝",
  "██████╔╝█████╗  ██║  ███╗█████╗  ██╔██╗ ██║   ██║   ",
  "██╔══██╗██╔══╝  ██║   ██║██╔══╝  ██║╚██╗██║   ██║   ",
  "██║  ██║███████╗╚██████╔╝███████╗██║ ╚████║   ██║   ",
  "╚═╝  ╚═╝╚══════╝ ╚═════╝ ╚══════╝╚═╝  ╚═══╝   ╚═╝   ",
];
