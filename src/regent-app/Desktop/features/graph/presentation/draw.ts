// Canvas 2D painter for the galaxy: dark space, thin low-alpha edges, then
// dots (colour by kind, radius by degree × zoom), and labels that fade in only
// once zoomed past a threshold so the far view stays a clean starfield. Pure
// drawing given a camera + the live layout array — no React, no DOM queries.
import type { Camera } from './camera';
import { worldToScreen } from './camera';
import type { LayoutNode } from '@/features/graph/viewmodels/useForceLayout';
import type { GraphEdge } from '@/features/graph/viewmodels/useGraphData';

// Two fields, one galaxy: near-black space in dark mode, bone daylight in
// light mode. Dot hues are mid-saturation and read on both.
const DARK = { bg: '#0a0b12', edge: 'rgba(150,165,210,', label: 'rgba(226,232,255,', ring: '#ffffff' };
const LIGHT = { bg: '#f2f0ea', edge: 'rgba(70,80,110,', label: 'rgba(35,40,60,', ring: '#1a1c26' };
const EDGE_BASE = 0.22; // resting edge alpha — thin but unmistakably present
const LABEL_K0 = 1.1; // below this: dots only
const LABEL_K1 = 1.9; // at/above this: labels fully in
const HOVER_GROW = 0.45; // extra radius fraction a fully-hovered node grows by
// Canvas `font` can't resolve CSS vars, so name the family concretely.
const FONT = '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif';

// Hex → rgb, mixed toward a target channel value (255 = lighten, 0 = darken).
// Small palette + ~40 nodes, so parsing per frame is free; no memo needed.
function shade(hex: string, t: number, toward: number): string {
  const h = hex.replace('#', '');
  const full = h.length === 3 ? h.split('').map((c) => c + c).join('') : h;
  const n = Number.parseInt(full, 16);
  const mix = (c: number): number => Math.round(c + (toward - c) * t);
  return `rgb(${mix((n >> 16) & 255)},${mix((n >> 8) & 255)},${mix(n & 255)})`;
}

export interface DrawParams {
  readonly ctx: CanvasRenderingContext2D;
  readonly width: number; // CSS px
  readonly height: number; // CSS px
  readonly dpr: number;
  readonly cam: Camera;
  readonly layout: readonly LayoutNode[];
  readonly edges: readonly GraphEdge[];
  readonly selectedId?: string;
  readonly dark: boolean;
  readonly colorOf: (kind: string) => string;
  readonly glyphOf: (kind: string) => string;
  /** Per-node hover progress (0→1), for the smooth grow-on-hover. Absent id = 0. */
  readonly hoverScales?: ReadonlyMap<string, number>;
}

const clamp01 = (v: number): number => Math.min(1, Math.max(0, v));

export function drawScene(p: DrawParams): void {
  const { ctx, width, height, dpr, cam, layout } = p;
  const theme = p.dark ? DARK : LIGHT;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.fillStyle = theme.bg;
  ctx.fillRect(0, 0, width, height);

  const byId = new Map<string, LayoutNode>();
  for (const n of layout) byId.set(n.id, n);

  // Edges under the dots — thin but present, a touch heavier when zoomed in.
  ctx.lineWidth = Math.max(1, Math.min(2.5, cam.k));
  for (const e of p.edges) {
    const a = byId.get(e.src);
    const b = byId.get(e.dst);
    if (!a || !b || a.x == null || a.y == null || b.x == null || b.y == null) continue;
    const s1 = worldToScreen(cam, a.x, a.y);
    const s2 = worldToScreen(cam, b.x, b.y);
    ctx.strokeStyle = `${theme.edge}${(EDGE_BASE + Math.min(0.25, (e.weight ?? 1) * 0.05)).toFixed(3)})`;
    ctx.beginPath();
    ctx.moveTo(s1.x, s1.y);
    ctx.lineTo(s2.x, s2.y);
    ctx.stroke();
  }

  const labelAlpha = clamp01((cam.k - LABEL_K0) / (LABEL_K1 - LABEL_K0));

  for (const n of layout) {
    if (n.x == null || n.y == null) continue;
    const s = worldToScreen(cam, n.x, n.y);
    // Screen-space floor: dots stay legible (≥6px) however far the galaxy is
    // zoomed out, so a wide graph never collapses into invisible specks.
    const hov = p.hoverScales?.get(n.id) ?? 0;
    const r = Math.max(6, n.radius * cam.k) * (1 + hov * HOVER_GROW);
    // Gradient in the node's OWN hue — a lit sphere: lighter at the top-left,
    // the base colour through the middle, a touch darker at the rim. Hovering
    // brightens it further so the grow reads as "lifting toward you".
    const base = p.colorOf(n.kind);
    const grad = ctx.createRadialGradient(s.x - r * 0.35, s.y - r * 0.35, r * 0.1, s.x, s.y, r);
    grad.addColorStop(0, shade(base, 0.5 + hov * 0.2, 255));
    grad.addColorStop(0.55, base);
    grad.addColorStop(1, shade(base, 0.22, 0));
    ctx.fillStyle = grad;
    ctx.beginPath();
    ctx.arc(s.x, s.y, r, 0, Math.PI * 2);
    ctx.fill();

    if (n.id === p.selectedId) {
      ctx.strokeStyle = theme.ring;
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.arc(s.x, s.y, r + 3, 0, Math.PI * 2);
      ctx.stroke();
    }

    if (labelAlpha > 0.02) {
      // Type glyph inside larger dots; the name label to the right.
      if (r >= 6) {
        ctx.fillStyle = `rgba(10,11,18,${labelAlpha.toFixed(3)})`;
        ctx.font = `${Math.round(r).toString()}px ${FONT}`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText(p.glyphOf(n.kind), s.x, s.y + 0.5);
      }
      ctx.fillStyle = `${theme.label}${labelAlpha.toFixed(3)})`;
      ctx.font = `11px ${FONT}`;
      ctx.textAlign = 'left';
      ctx.textBaseline = 'middle';
      ctx.fillText(truncate(n.name), s.x + r + 4, s.y);
    }
  }
}

function truncate(name: string): string {
  return name.length > 28 ? `${name.slice(0, 27)}…` : name;
}
