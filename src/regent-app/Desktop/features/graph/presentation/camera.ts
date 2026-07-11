// Pan/zoom camera math for the graph canvas — pure, no DOM. The transform is
// `screen = world * k + offset`; `offset` is the pan in CSS px, `k` the zoom.
// Kept separate from GraphCanvas so the screen↔world round-trip is unit-tested.

export interface Camera {
  readonly x: number;
  readonly y: number;
  readonly k: number;
}

export const K_MIN = 0.05;
export const K_MAX = 8;

export const clampK = (k: number): number => Math.min(K_MAX, Math.max(K_MIN, k));

export interface Point {
  readonly x: number;
  readonly y: number;
}

export function screenToWorld(cam: Camera, sx: number, sy: number): Point {
  return { x: (sx - cam.x) / cam.k, y: (sy - cam.y) / cam.k };
}

export function worldToScreen(cam: Camera, wx: number, wy: number): Point {
  return { x: wx * cam.k + cam.x, y: wy * cam.k + cam.y };
}

/** Zoom by `factor` while keeping the world point under (sx,sy) pinned to the
 * cursor — the standard zoom-toward-pointer behaviour. */
export function zoomAt(cam: Camera, sx: number, sy: number, factor: number): Camera {
  const k = clampK(cam.k * factor);
  const world = screenToWorld(cam, sx, sy);
  return { k, x: sx - world.x * k, y: sy - world.y * k };
}

/** Camera that centres world point (wx,wy) in a viewport of w×h at zoom k. */
export function centerOn(wx: number, wy: number, k: number, w: number, h: number): Camera {
  return { k, x: w / 2 - wx * k, y: h / 2 - wy * k };
}

/** Frame a set of world points into w×h. Two constraints, whichever is tighter:
 * (1) the whole graph must fit inside the padded viewport, and (2) a typical
 * node shouldn't render bigger than `targetNodePx`. Zooming by node size (not
 * just bounding box) is what makes a 5-node graph and a 500-node graph read at
 * the SAME apparent scale — a tight cluster fills comfortably instead of
 * becoming a speck, and a dense field isn't slammed in. `refRadius` is a
 * representative (median) node radius. Empty/degenerate input → a sane default. */
export function fitToContent(
  points: readonly Point[],
  w: number,
  h: number,
  refRadius = 12,
  pad = 90,
  targetNodePx = 13,
): Camera {
  if (points.length === 0 || w === 0 || h === 0) return centerOn(0, 0, 1, w, h);
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const p of points) {
    if (p.x < minX) minX = p.x;
    if (p.y < minY) minY = p.y;
    if (p.x > maxX) maxX = p.x;
    if (p.y > maxY) maxY = p.y;
  }
  const spanX = Math.max(1, maxX - minX);
  const spanY = Math.max(1, maxY - minY);
  const kFit = Math.min((w - pad * 2) / spanX, (h - pad * 2) / spanY);
  const kNode = targetNodePx / Math.max(1, refRadius);
  const k = clampK(Math.min(kFit, kNode));
  return centerOn((minX + maxX) / 2, (minY + maxY) / 2, k, w, h);
}
