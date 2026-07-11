'use client';
// Shared pointer-capture gesture for FloatingWindow's drag (header) and
// resize (corner grip): capture on down, push the geometry through
// immediately while the pointer is held — easing is the spring's job on
// settle, not this hook's — then commit the final value to the registry on
// release. One get/set geometry seam so both gestures share a single clamp
// shape instead of duplicating the ref-origin dance twice.
import { useRef, type PointerEvent } from 'react';

const MARGIN = 8;
const MIN_WIDTH = 220;
const MIN_HEIGHT = 140;

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

export interface DragResizeGeometry {
  getX: () => number;
  getY: () => number;
  getWidth: () => number;
  getHeight: () => number;
  setPosition: (x: number, y: number) => void;
  setSize: (width: number, height: number) => void;
}

export interface DragResizeHandlers {
  onPointerDown: (e: PointerEvent<HTMLDivElement>) => void;
  onPointerMove: (e: PointerEvent<HTMLDivElement>) => void;
  onPointerUp: (e: PointerEvent<HTMLDivElement>) => void;
}

export function useDragResize(
  geo: DragResizeGeometry,
  onFocus: () => void,
  onMove: (x: number, y: number) => void,
  onResize?: (width: number, height: number) => void,
): { drag: DragResizeHandlers; grip: DragResizeHandlers } {
  const dragOrigin = useRef<{ sx: number; sy: number; ox: number; oy: number } | null>(null);
  const resizeOrigin = useRef<{ sx: number; sy: number; ow: number; oh: number } | null>(null);

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    // Never capture from interactive children — pointer capture retargets the
    // rest of the gesture to the header, which ate the close button's click.
    if ((e.target as HTMLElement).closest('button') !== null) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    dragOrigin.current = { sx: e.clientX, sy: e.clientY, ox: geo.getX(), oy: geo.getY() };
    onFocus();
  };
  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const d = dragOrigin.current;
    if (!d) return;
    const nx = clamp(d.ox + e.clientX - d.sx, MARGIN, window.innerWidth - geo.getWidth() - MARGIN);
    const ny = clamp(d.oy + e.clientY - d.sy, MARGIN, window.innerHeight - 80);
    geo.setPosition(nx, ny);
  };
  const onPointerUp = () => {
    if (!dragOrigin.current) return;
    dragOrigin.current = null;
    onMove(geo.getX(), geo.getY());
  };

  const onGripDown = (e: PointerEvent<HTMLDivElement>) => {
    // Don't let the outer panel's onPointerDown re-fire focus/drag setup.
    e.stopPropagation();
    e.currentTarget.setPointerCapture(e.pointerId);
    resizeOrigin.current = { sx: e.clientX, sy: e.clientY, ow: geo.getWidth(), oh: geo.getHeight() };
    onFocus();
  };
  const onGripMove = (e: PointerEvent<HTMLDivElement>) => {
    const r = resizeOrigin.current;
    if (!r) return;
    const nw = clamp(r.ow + e.clientX - r.sx, MIN_WIDTH, window.innerWidth - geo.getX() - MARGIN);
    const nh = clamp(r.oh + e.clientY - r.sy, MIN_HEIGHT, window.innerHeight - geo.getY() - MARGIN);
    geo.setSize(nw, nh);
  };
  const onGripUp = () => {
    if (!resizeOrigin.current) return;
    resizeOrigin.current = null;
    onResize?.(geo.getWidth(), geo.getHeight());
  };

  return {
    drag: { onPointerDown, onPointerMove, onPointerUp },
    grip: { onPointerDown: onGripDown, onPointerMove: onGripMove, onPointerUp: onGripUp },
  };
}
