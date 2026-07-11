'use client';
// The galaxy renderer. One rAF loop paints the live layout every frame reading
// positions/camera/selection from refs — zero React state per frame. Wheel
// zooms toward the cursor, drag pans, a click hit-tests to the nearest dot.
// `focusNode` GSAP-tweens the camera to centre a node (jumps under reduced
// motion). Camera/draw math lives in ./camera and ./draw to keep this lean.
import { useEffect, useImperativeHandle, useRef, type Ref } from 'react';
import gsap from 'gsap';
import { type Camera, centerOn, fitToContent, screenToWorld, zoomAt } from './camera';
import { drawScene } from './draw';
import { kindColor, kindGlyph, type GraphEdge } from '@/features/graph/viewmodels/useGraphData';
import type { ForceLayout, LayoutNode } from '@/features/graph/viewmodels/useForceLayout';

export interface GraphCanvasHandle {
  focusNode(id: string): void;
}

interface Props {
  layoutRef: React.RefObject<LayoutNode[]>;
  simRef: ForceLayout['simRef'];
  edges: readonly GraphEdge[];
  selectedId?: string;
  onSelect: (id: string) => void;
  ariaLabel: string;
  ref?: Ref<GraphCanvasHandle>;
}

const FOCUS_K = 1.6; // comfortable zoom that also brings labels in
const CLICK_SLOP = 4; // px of movement below which a pointer-up counts as a click

// Cheap per-frame theme probe: the data-theme attribute wins; absent means the
// OS preference drives (mirrors shared/state/theme.ts). Reading .matches off a
// cached MediaQueryList costs nothing.
const systemDark =
  typeof matchMedia === 'function' ? matchMedia('(prefers-color-scheme: dark)') : undefined;
function isDarkTheme(): boolean {
  const mode = document.documentElement.getAttribute('data-theme');
  if (mode === 'dark') return true;
  if (mode === 'light') return false;
  return systemDark?.matches ?? true;
}

export function GraphCanvas({ layoutRef, simRef, edges, selectedId, onSelect, ariaLabel, ref }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const cam = useRef<Camera>({ x: 0, y: 0, k: 0.9 });
  const size = useRef({ w: 0, h: 0, dpr: 1 });
  const fitted = useRef(false);
  const firstSeen = useRef(0); // frame count since layout first had nodes
  const edgesRef = useRef(edges);
  const selRef = useRef(selectedId);
  edgesRef.current = edges;
  selRef.current = selectedId;
  // Hover: the id under the cursor, plus a per-node scale (0→1) that eases
  // toward 1 for the hovered node and back to 0 for the one just left — so the
  // grow/shrink is fluid, and moving between two nodes cross-fades. Map holds
  // only in-flight entries, so it's tiny.
  const hoverId = useRef<string | undefined>(undefined);
  const hoverScales = useRef<Map<string, number>>(new Map());
  const hoverVel = useRef<Map<string, number>>(new Map()); // spring velocity per node
  const reduceMotion =
    typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;

  const nodeAt = (sx: number, sy: number): LayoutNode | undefined => {
    const w = screenToWorld(cam.current, sx, sy);
    let best: LayoutNode | undefined;
    let bestD = Infinity;
    for (const n of layoutRef.current) {
      if (n.x == null || n.y == null) continue;
      const d = Math.hypot(n.x - w.x, n.y - w.y);
      const hit = n.radius + 4 / cam.current.k;
      if (d <= hit && d < bestD) {
        best = n;
        bestD = d;
      }
    }
    return best;
  };

  useImperativeHandle(
    ref,
    (): GraphCanvasHandle => ({
      focusNode: (id) => {
        const n = layoutRef.current.find((m) => m.id === id);
        if (!n || n.x == null || n.y == null) return;
        const target = centerOn(n.x, n.y, Math.max(cam.current.k, FOCUS_K), size.current.w, size.current.h);
        gsap.killTweensOf(cam.current);
        if (matchMedia('(prefers-reduced-motion: reduce)').matches) {
          Object.assign(cam.current, target);
        } else {
          gsap.to(cam.current, { ...target, duration: 0.6, ease: 'power2.inOut' });
        }
      },
    }),
    [layoutRef],
  );

  // Single rAF loop — reads everything from refs so it never restarts.
  useEffect(() => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext('2d');
    if (!canvas || !ctx) return;
    let raf = 0;
    const frame = () => {
      raf = requestAnimationFrame(frame);
      const { w, h, dpr } = size.current;
      if (w === 0 || h === 0) return;
      const layout = layoutRef.current;
      // Frame the whole galaxy once the force sim has spread it (~60 frames ≈
      // 1s of ticks) — fitting at frame 0 would zoom into the seed spiral.
      if (!fitted.current && layout.length > 0) {
        firstSeen.current += 1;
        if (firstSeen.current > 60) {
          const pts = layout.filter((n) => n.x != null && n.y != null).map((n) => ({ x: n.x!, y: n.y! }));
          Object.assign(cam.current, fitToContent(pts, w, h));
          fitted.current = true;
        }
      }
      // Spring hover scales toward their targets (1 for the hovered node, 0 for
      // the rest) — a critically-damped-ish spring gives a fluid grow with a
      // hint of overshoot, far more alive than a linear ease. Entries that have
      // fully settled at 0 are dropped so the maps stay tiny.
      const hv = hoverScales.current;
      const vel = hoverVel.current;
      const hid = hoverId.current;
      if (hid !== undefined && !hv.has(hid)) hv.set(hid, 0);
      const STIFF = 0.14, DAMP = 0.76;
      for (const [id, x] of hv) {
        const target = id === hid ? 1 : 0;
        if (reduceMotion) {
          if (target === 0) { hv.delete(id); vel.delete(id); } else hv.set(id, target);
          continue;
        }
        const v = (vel.get(id) ?? 0) + (target - x) * STIFF;
        const nv = v * DAMP;
        const nx = x + nv;
        if (target === 0 && Math.abs(nx) < 0.004 && Math.abs(nv) < 0.004) {
          hv.delete(id);
          vel.delete(id);
        } else {
          hv.set(id, nx);
          vel.set(id, nv);
        }
      }
      drawScene({
        ctx, width: w, height: h, dpr,
        cam: cam.current, layout, edges: edgesRef.current,
        selectedId: selRef.current, dark: isDarkTheme(), colorOf: kindColor, glyphOf: kindGlyph,
        hoverScales: hv,
      });
    };
    frame();
    return () => cancelAnimationFrame(raf);
  }, [layoutRef]);

  // Backing-store sizing at the device pixel ratio, kept in sync on resize.
  useEffect(() => {
    const canvas = canvasRef.current;
    const container = containerRef.current;
    if (!canvas || !container) return;
    const apply = () => {
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      const w = container.clientWidth;
      const h = container.clientHeight;
      size.current = { w, h, dpr };
      canvas.width = Math.round(w * dpr);
      canvas.height = Math.round(h * dpr);
    };
    apply();
    const ro = new ResizeObserver(apply);
    ro.observe(container);
    return () => ro.disconnect();
  }, []);

  // Zoom toward cursor — non-passive so we can preventDefault the page.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const rect = canvas.getBoundingClientRect();
      const factor = e.deltaY < 0 ? 1.1 : 1 / 1.1;
      Object.assign(cam.current, zoomAt(cam.current, e.clientX - rect.left, e.clientY - rect.top, factor));
    };
    canvas.addEventListener('wheel', onWheel, { passive: false });
    return () => canvas.removeEventListener('wheel', onWheel);
  }, []);

  // A drag is either grabbing a NODE (pin it to the cursor, reheat the sim so
  // the links spring its neighbours) or panning empty space.
  const drag = useRef<{ x: number; y: number; moved: number; node?: LayoutNode } | null>(null);

  const onPointerDown = (e: React.PointerEvent) => {
    (e.target as Element).setPointerCapture(e.pointerId);
    const rect = e.currentTarget.getBoundingClientRect();
    const node = nodeAt(e.clientX - rect.left, e.clientY - rect.top);
    drag.current = { x: e.clientX, y: e.clientY, moved: 0, node };
    if (node) {
      // Warm the sim and pin the grabbed node under the cursor — the elastic
      // web is just the links pulling everything else toward the pinned node.
      simRef.current?.alphaTarget(0.3).restart();
      node.fx = node.x;
      node.fy = node.y;
      hoverId.current = node.id; // keep it grown while held
    }
  };
  const onPointerMove = (e: React.PointerEvent) => {
    const d = drag.current;
    const rect = e.currentTarget.getBoundingClientRect();
    if (!d) {
      // Not dragging → track the hovered node (drives the grow) and the cursor.
      const hit = nodeAt(e.clientX - rect.left, e.clientY - rect.top);
      hoverId.current = hit?.id;
      (e.currentTarget as HTMLElement).style.cursor = hit ? 'grab' : 'default';
      return;
    }
    const dx = e.clientX - d.x;
    const dy = e.clientY - d.y;
    d.moved += Math.abs(dx) + Math.abs(dy);
    d.x = e.clientX;
    d.y = e.clientY;
    if (d.node) {
      // Steer the pinned node in WORLD space; the sim ticks its neighbours
      // along elastically every frame (the "strings" springing).
      const w = screenToWorld(cam.current, e.clientX - rect.left, e.clientY - rect.top);
      d.node.fx = w.x;
      d.node.fy = w.y;
      (e.currentTarget as HTMLElement).style.cursor = 'grabbing';
      return;
    }
    // Empty-space pan. Mutate in place (not replace) so an in-flight GSAP focus
    // tween and the rAF loop keep sharing one camera object.
    gsap.killTweensOf(cam.current);
    Object.assign(cam.current, { x: cam.current.x + dx, y: cam.current.y + dy });
  };
  const onPointerLeave = () => {
    hoverId.current = undefined;
  };
  const onPointerUp = (e: React.PointerEvent) => {
    const d = drag.current;
    drag.current = null;
    if (!d) return;
    if (d.node) {
      // Release the pin and let the sim settle back to rest.
      d.node.fx = null;
      d.node.fy = null;
      simRef.current?.alphaTarget(0);
      hoverId.current = undefined;
      (e.currentTarget as HTMLElement).style.cursor = 'default';
      // A grab that barely moved is a click → select.
      if (d.moved <= CLICK_SLOP) onSelect(d.node.id);
    }
  };

  return (
    <div ref={containerRef} className="relative h-full w-full overflow-hidden">
      <canvas
        ref={canvasRef}
        role="img"
        aria-label={ariaLabel}
        className="h-full w-full touch-none select-none"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerLeave={onPointerLeave}
      />
    </div>
  );
}
