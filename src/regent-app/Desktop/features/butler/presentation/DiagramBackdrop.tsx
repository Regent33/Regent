'use client';
// Butler's diagram stage follows Regent's effective light/dark theme and
// reveals the Mermaid visual before narration starts. The viewport supports
// pointer, touch, wheel, button, and keyboard pan/zoom without rerendering the
// Mermaid source itself. A render failure fails open so speech never deadlocks.
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import gsap from 'gsap';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { CloseIcon, MinusIcon, PlusIcon } from '@/shared/ui/icons';
import { renderMermaid } from '@/shared/ui/markdown/mermaidLoader';
import { specToMermaid } from '@/shared/diagram/diagramMermaid';
import { useTheme } from '@/shared/state/theme';
import {
  INITIAL_DIAGRAM_VIEW,
  panDiagram,
  zoomDiagramAt,
  type DiagramView,
} from '@/features/butler/domain/diagramViewport';
import type { PresentSpec } from '@/shared/diagram/presentSpec';

const REVEAL_SELECTOR = '.node, .edgePath, .edgeLabel, section, .timeline-event, .mindmap-node';
let warned = false;

type Gesture =
  | { kind: 'pan'; px: number; py: number; view: DiagramView }
  | { kind: 'pinch'; distance: number; mx: number; my: number; view: DiagramView };

function distance(a: { x: number; y: number }, b: { x: number; y: number }): number {
  return Math.hypot(b.x - a.x, b.y - a.y);
}

function midpoint(a: { x: number; y: number }, b: { x: number; y: number }) {
  return { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
}

function useEffectiveMermaidTheme(): 'default' | 'dark' {
  const { mode } = useTheme();
  const [systemDark, setSystemDark] = useState(
    () => typeof matchMedia === 'function' && matchMedia('(prefers-color-scheme: dark)').matches,
  );

  useEffect(() => {
    if (typeof matchMedia !== 'function') return;
    const query = matchMedia('(prefers-color-scheme: dark)');
    const sync = () => setSystemDark(query.matches);
    query.addEventListener('change', sync);
    return () => query.removeEventListener('change', sync);
  }, []);

  return mode === 'dark' || (mode === 'system' && systemDark) ? 'dark' : 'default';
}

export function DiagramBackdrop({
  spec,
  onDismiss,
  onFail,
  onReady,
}: {
  spec: PresentSpec;
  onDismiss: () => void;
  onFail?: () => void;
  onReady: () => void;
}) {
  const hostRef = useRef<HTMLDivElement>(null);
  const viewportRef = useRef<HTMLDivElement>(null);
  const [svg, setSvg] = useState<string | null>(null);
  const [view, setViewState] = useState<DiagramView>(INITIAL_DIAGRAM_VIEW);
  const viewRef = useRef<DiagramView>(INITIAL_DIAGRAM_VIEW);
  const pointersRef = useRef(new Map<number, { x: number; y: number }>());
  const gestureRef = useRef<Gesture | null>(null);
  const mermaidTheme = useEffectiveMermaidTheme();
  const s = t().butler;

  const setView = useCallback((next: DiagramView) => {
    viewRef.current = next;
    setViewState(next);
  }, []);

  const resetView = useCallback(() => setView(INITIAL_DIAGRAM_VIEW), [setView]);

  const zoomAtViewportPoint = useCallback(
    (clientX: number, clientY: number, factor: number) => {
      const rect = viewportRef.current?.getBoundingClientRect();
      if (!rect) return;
      const fx = clientX - (rect.left + rect.width / 2);
      const fy = clientY - (rect.top + rect.height / 2);
      setView(zoomDiagramAt(viewRef.current, fx, fy, factor));
    },
    [setView],
  );

  const zoomFromCenter = useCallback(
    (factor: number) => setView(zoomDiagramAt(viewRef.current, 0, 0, factor)),
    [setView],
  );

  useEffect(() => {
    let alive = true;
    setSvg(null);
    resetView();
    void renderMermaid(specToMermaid(spec), mermaidTheme).then(
      (out) => alive && setSvg(out),
      (err: unknown) => {
        if (!warned) {
          console.warn('[butler] diagram render failed - falling back to voice', err);
          warned = true;
        }
        if (alive) {
          setSvg(null);
          onReady();
          onFail?.();
        }
      },
    );
    return () => {
      alive = false;
    };
  }, [spec, mermaidTheme, onFail, onReady, resetView]);

  // Pin every reveal target before paint, then release narration only after the
  // fully visible diagram has painted. Reduced motion skips the stagger.
  useLayoutEffect(() => {
    const host = hostRef.current;
    if (!host || svg === null) return;
    const found = host.querySelectorAll<SVGElement>(REVEAL_SELECTOR);
    const targets = found.length > 0 ? Array.from(found) : Array.from(host.querySelectorAll<SVGElement>('svg > g'));
    let frame1 = 0;
    let frame2 = 0;
    const readyAfterPaint = () => {
      frame1 = requestAnimationFrame(() => {
        frame2 = requestAnimationFrame(onReady);
      });
    };
    if (targets.length === 0) {
      readyAfterPaint();
      return () => {
        cancelAnimationFrame(frame1);
        cancelAnimationFrame(frame2);
      };
    }
    const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
    if (reduced) {
      gsap.set(targets, { opacity: 1 });
      readyAfterPaint();
      return () => {
        cancelAnimationFrame(frame1);
        cancelAnimationFrame(frame2);
      };
    }
    const stagger = targets.length > 1 ? Math.min(0.035, 0.24 / (targets.length - 1)) : 0;
    const tween = gsap.fromTo(
      targets,
      { opacity: 0, y: 4 },
      { opacity: 1, y: 0, duration: 0.16, stagger, ease: 'power1.out', onComplete: readyAfterPaint },
    );
    return () => {
      tween.kill();
      cancelAnimationFrame(frame1);
      cancelAnimationFrame(frame2);
    };
  }, [svg, onReady]);

  if (svg === null) return null;

  const beginGesture = () => {
    const points = Array.from(pointersRef.current.values());
    if (points.length >= 2) {
      const mid = midpoint(points[0], points[1]);
      gestureRef.current = {
        kind: 'pinch',
        distance: Math.max(1, distance(points[0], points[1])),
        mx: mid.x,
        my: mid.y,
        view: viewRef.current,
      };
    } else if (points.length === 1) {
      gestureRef.current = { kind: 'pan', px: points[0].x, py: points[0].y, view: viewRef.current };
    } else {
      gestureRef.current = null;
    }
  };

  const onPointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    if (event.pointerType === 'mouse' && event.button !== 0) return;
    pointersRef.current.set(event.pointerId, { x: event.clientX, y: event.clientY });
    event.currentTarget.setPointerCapture(event.pointerId);
    beginGesture();
  };

  const onPointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    if (!pointersRef.current.has(event.pointerId)) return;
    pointersRef.current.set(event.pointerId, { x: event.clientX, y: event.clientY });
    const gesture = gestureRef.current;
    const points = Array.from(pointersRef.current.values());
    if (!gesture) return;
    if (gesture.kind === 'pinch' && points.length >= 2) {
      const mid = midpoint(points[0], points[1]);
      const factor = distance(points[0], points[1]) / gesture.distance;
      const rect = viewportRef.current?.getBoundingClientRect();
      if (!rect) return;
      const fx = gesture.mx - (rect.left + rect.width / 2);
      const fy = gesture.my - (rect.top + rect.height / 2);
      const zoomed = zoomDiagramAt(gesture.view, fx, fy, factor);
      setView(panDiagram(zoomed, mid.x - gesture.mx, mid.y - gesture.my));
    } else if (gesture.kind === 'pan' && points.length === 1) {
      setView(panDiagram(gesture.view, event.clientX - gesture.px, event.clientY - gesture.py));
    }
  };

  const onPointerEnd = (event: React.PointerEvent<HTMLDivElement>) => {
    pointersRef.current.delete(event.pointerId);
    beginGesture();
  };

  const onKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    const panStep = event.shiftKey ? 96 : 48;
    let next: DiagramView | undefined;
    if (event.key === '+' || event.key === '=') next = zoomDiagramAt(viewRef.current, 0, 0, 1.2);
    else if (event.key === '-') next = zoomDiagramAt(viewRef.current, 0, 0, 1 / 1.2);
    else if (event.key === '0') next = INITIAL_DIAGRAM_VIEW;
    else if (event.key === 'ArrowLeft') next = panDiagram(viewRef.current, panStep, 0);
    else if (event.key === 'ArrowRight') next = panDiagram(viewRef.current, -panStep, 0);
    else if (event.key === 'ArrowUp') next = panDiagram(viewRef.current, 0, panStep);
    else if (event.key === 'ArrowDown') next = panDiagram(viewRef.current, 0, -panStep);
    if (!next) return;
    event.preventDefault();
    setView(next);
  };

  return (
    <div
      className="absolute inset-0 overflow-hidden bg-bg"
      style={{ background: 'radial-gradient(ellipse 92% 88% at 50% 45%, var(--surface) 0%, var(--bg) 72%)' }}
    >
      <div
        ref={viewportRef}
        role="region"
        tabIndex={0}
        aria-label={s.diagramViewport}
        aria-describedby="butler-diagram-help"
        className="absolute inset-0 touch-none overflow-hidden overscroll-none outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent [&:active]:cursor-grabbing [&:not(:active)]:cursor-grab"
        onWheel={(event) => {
          event.preventDefault();
          zoomAtViewportPoint(event.clientX, event.clientY, Math.exp(-event.deltaY * 0.0015));
        }}
        onDoubleClick={resetView}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerEnd}
        onPointerCancel={onPointerEnd}
        onKeyDown={onKeyDown}
      >
        <span id="butler-diagram-help" className="sr-only">{s.diagramInteractionHelp}</span>
        <div className="flex h-full items-center justify-center px-20 pb-44 pt-48">
          <div
            className="will-change-transform"
            style={{ transform: `translate3d(${view.x}px, ${view.y}px, 0) scale(${view.k})` }}
          >
            <div
              ref={hostRef}
              className="w-[82vw] [&_svg]:mx-auto [&_svg]:h-auto [&_svg]:max-h-[58vh] [&_svg]:max-w-full"
              dangerouslySetInnerHTML={{ __html: svg }}
            />
          </div>
        </div>
      </div>
      <div className="absolute left-1/2 top-14 z-10 -translate-x-1/2">
        <Button variant="secondary" size="sm" aria-label={s.diagramDismiss} onClick={onDismiss}>
          <CloseIcon className="size-3.5" />
          {s.diagramDismiss}
        </Button>
      </div>
      <h2 className="pointer-events-none absolute left-1/2 top-32 z-10 max-w-[80vw] -translate-x-1/2 text-center text-2xl font-semibold text-text-primary">
        {spec.title}
      </h2>
      <div
        role="group"
        aria-label={s.diagramControls}
        className="absolute bottom-6 right-6 z-10 flex flex-col overflow-hidden rounded-lg border border-stroke-secondary bg-surface shadow-[var(--shadow-elev)]"
      >
        <Button variant="ghost" size="icon" aria-label={s.diagramZoomIn} onClick={() => zoomFromCenter(1.2)}>
          <PlusIcon />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          aria-label={s.diagramResetView}
          title={s.diagramResetView}
          onClick={resetView}
          className="border-y border-stroke-secondary font-mono text-[10px] tabular-nums"
        >
          {Math.round(view.k * 100)}%
        </Button>
        <Button variant="ghost" size="icon" aria-label={s.diagramZoomOut} onClick={() => zoomFromCenter(1 / 1.2)}>
          <MinusIcon />
        </Button>
      </div>
    </div>
  );
}
