// Regent's terminal palette. Teal is the accent; silver/white is the main
// tone, rendered as a top-to-bottom gradient across a silver ramp. Truecolor
// hex (Ink renders 24-bit) lets us use the exact brand teal instead of the
// 256-color approximation the Go surface settled for.

export const palette = {
  teal: "#00A19B", // brand accent (mandated): headings, spinner, prompt
  tealDim: "#0A6F6B", // dim teal for labels/keys
  gold: "#F5A300", // the king's crown (amber-gold, per the canonical mark)
  goldBright: "#FFD24A", // crown highlight (top of the gradient)
  silver: "#E4DDD3", // brand silver (warm off-white): panel borders / mid tone
  white: "#FFFFFF",
  grey: "#8A8A8A",
} as const;

// Warm silver gradient anchored on the brand silver (#E4DDD3): bright warm-white
// → dim warm-grey. Gradient lines index into this ramp.
export const silverRamp = [
  "#F6F2EB",
  "#ECE6DB",
  "#E4DDD3",
  "#D8D0C3",
  "#CBC2B2",
  "#BBB1A0",
  "#AAA08D",
  "#998F7C",
] as const;

/** Map row `i` of `n` into the silver ramp (clamped). */
export function shade(i: number, n: number): string {
  if (n <= 1) return silverRamp[0];
  const idx = Math.min(Math.floor((i * (silverRamp.length - 1)) / (n - 1)), silverRamp.length - 1);
  return silverRamp[idx] ?? silverRamp[0];
}

// Teal gradient (light → brand → deep) for the "REGENT" wordmark.
export const tealRamp = ["#5FD3CD", "#19B3AC", "#00A19B", "#0B8782"] as const;

/** Map row `i` of `n` into the teal ramp (clamped). */
export function tealShade(i: number, n: number): string {
  if (n <= 1) return tealRamp[0];
  const idx = Math.min(Math.floor((i * (tealRamp.length - 1)) / (n - 1)), tealRamp.length - 1);
  return tealRamp[idx] ?? tealRamp[0];
}
