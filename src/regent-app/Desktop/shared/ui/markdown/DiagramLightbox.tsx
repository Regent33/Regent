'use client';
// Full-screen diagram viewer: the rendered mermaid SVG, large, with wheel-zoom
// (around the cursor), pointer-drag pan, and double-click to reset. Fits to the
// stage on open. Esc, the close button, or a click on the empty scrim closes;
// dragging over the diagram pans instead of closing. Scrim/fade mirror
// ZoomableImage. No portal — a fixed overlay escapes chat's overflow already.
import { useCallback, useEffect, useRef, useState } from 'react';
import { t } from '@/shared/i18n/t';
import { renderMermaid } from '@/shared/ui/markdown/mermaidLoader';

interface View {
  x: number;
  y: number;
  k: number;
}

const MIN_K = 0.2;
const MAX_K = 8;
const clampK = (k: number) => Math.min(MAX_K, Math.max(MIN_K, k));

export function DiagramLightbox({ code, onClose }: { code: string; onClose: () => void }) {
  const s = t().chat.markdown;
  const [svg, setSvg] = useState<string | undefined>(undefined);
  const [view, setView] = useState<View | undefined>(undefined); // undefined until fitted (no flash)
  const stageRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const drag = useRef<{ px: number; py: number; ox: number; oy: number } | null>(null);

  useEffect(() => {
    let alive = true;
    renderMermaid(code).then(
      (r) => alive && setSvg(r),
      () => onClose(), // unrenderable → nothing to show
    );
    return () => {
      alive = false;
    };
  }, [code, onClose]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && onClose();
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [onClose]);

  // Fit-to-stage + centre once the SVG has laid out (offsetW/H ignore transform).
  const fit = useCallback(() => {
    const stage = stageRef.current;
    const content = contentRef.current;
    if (!stage || !content) return;
    const cw = content.offsetWidth;
    const ch = content.offsetHeight;
    if (cw === 0 || ch === 0) return;
    const k = clampK(Math.min((stage.clientWidth * 0.9) / cw, (stage.clientHeight * 0.9) / ch, 2));
    setView({ k, x: (stage.clientWidth - cw * k) / 2, y: (stage.clientHeight - ch * k) / 2 });
  }, []);

  useEffect(() => {
    if (svg !== undefined) fit();
  }, [svg, fit]);

  const onWheel = useCallback((e: React.WheelEvent) => {
    const rect = stageRef.current?.getBoundingClientRect();
    if (!rect) return;
    const cx = e.clientX - rect.left;
    const cy = e.clientY - rect.top;
    setView((v) => {
      if (!v) return v;
      const k = clampK(v.k * (e.deltaY < 0 ? 1.12 : 1 / 1.12));
      const r = k / v.k; // keep the point under the cursor fixed
      return { k, x: cx - (cx - v.x) * r, y: cy - (cy - v.y) * r };
    });
  }, []);

  const onPointerDown = (e: React.PointerEvent) => {
    if (!view) return;
    drag.current = { px: e.clientX, py: e.clientY, ox: view.x, oy: view.y };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  };
  const onPointerMove = (e: React.PointerEvent) => {
    const d = drag.current;
    if (!d) return;
    setView((v) => (v ? { ...v, x: d.ox + (e.clientX - d.px), y: d.oy + (e.clientY - d.py) } : v));
  };
  const endDrag = () => {
    drag.current = null;
  };

  return (
    <div
      role="presentation"
      className="fixed inset-0 z-50 flex flex-col bg-scrim backdrop-blur-[2px] motion-safe:animate-[fadeIn_120ms_ease-out]"
      onClick={onClose}
    >
      <div className="flex shrink-0 justify-end p-3">
        <button
          type="button"
          aria-label={s.closeDiagram}
          onClick={onClose}
          className="rounded-md p-1.5 text-text-secondary transition-colors hover:bg-stroke-secondary hover:text-text-primary"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="size-5">
            <path d="M6 6l12 12M18 6L6 18" strokeLinecap="round" />
          </svg>
        </button>
      </div>
      <div
        ref={stageRef}
        className="relative flex-1 touch-none overflow-hidden [&:active]:cursor-grabbing [&:not(:active)]:cursor-grab"
        onClick={(e) => e.stopPropagation()} // interacting with the stage never closes
        onDoubleClick={fit}
        onWheel={onWheel}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
      >
        {svg !== undefined && (
          <div
            ref={contentRef}
            className="absolute left-0 top-0 origin-top-left will-change-transform [&_svg]:h-auto [&_svg]:max-w-none"
            style={{
              transform: view ? `translate(${view.x}px, ${view.y}px) scale(${view.k})` : undefined,
              opacity: view ? 1 : 0,
            }}
            dangerouslySetInnerHTML={{ __html: svg }}
          />
        )}
      </div>
    </div>
  );
}
