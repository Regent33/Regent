'use client';
// The diagram stage — a full-bleed dark backdrop behind the Butler call that
// renders a ```present spec as a NotebookLM-style diagram. mermaid draws it
// (dark theme, securityLevel 'strict' → safe to inject), and while Regent is
// still narrating the shapes fade in one after another (GSAP stagger). Any
// parse/render failure is silent: render nothing and tell the parent to fall
// back to the voice mark. The exit fade lives in ButlerView's wrapper.
import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import gsap from 'gsap';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { CloseIcon } from '@/shared/ui/icons';
import { renderMermaid } from '@/shared/ui/markdown/mermaidLoader';
import { specToMermaid } from '@/features/butler/presentation/diagramMermaid';
import type { PresentSpec } from '@/features/butler/data/presentSpec';

// Top-level shape groups mermaid emits, per diagram kind — whichever set is
// non-empty is what we stagger.
const REVEAL_SELECTOR = '.node, .edgePath, .edgeLabel, section, .timeline-event, .mindmap-node';
let warned = false;

export function DiagramBackdrop({
  spec,
  speaking,
  onDismiss,
  onFail,
}: {
  spec: PresentSpec;
  speaking: boolean;
  onDismiss: () => void;
  onFail?: () => void;
}) {
  const hostRef = useRef<HTMLDivElement>(null);
  const [svg, setSvg] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    void renderMermaid(specToMermaid(spec), 'dark').then(
      (out) => alive && setSvg(out),
      (err: unknown) => {
        if (!warned) {
          console.warn('[butler] diagram render failed — falling back to voice', err);
          warned = true;
        }
        if (alive) {
          setSvg(null);
          onFail?.();
        }
      },
    );
    return () => {
      alive = false;
    };
  }, [spec, onFail]);

  // Staged reveal. useLayoutEffect so opacity is pinned to 0 BEFORE the browser
  // paints the freshly-injected SVG (no full-diagram flash). Not speaking or
  // reduced-motion → everything is shown at once.
  useLayoutEffect(() => {
    const host = hostRef.current;
    if (!host || svg === null) return;
    const found = host.querySelectorAll<SVGElement>(REVEAL_SELECTOR);
    const targets = found.length > 0 ? Array.from(found) : Array.from(host.querySelectorAll<SVGElement>('svg > g'));
    if (targets.length === 0) return;
    const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
    if (!speaking || reduced) {
      gsap.set(targets, { opacity: 1 });
      return;
    }
    const tween = gsap.fromTo(targets, { opacity: 0 }, { opacity: 1, duration: 0.35, stagger: 0.35, ease: 'power1.out' });
    return () => void tween.kill();
  }, [svg, speaking]);

  if (svg === null) return null;

  return (
    <div className="absolute inset-0 overflow-auto bg-[rgba(6,8,14,0.78)]">
      {/* Subtler than the globe's space vignette — the diagram floats centred. */}
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0"
        style={{ background: 'radial-gradient(ellipse 92% 88% at 50% 45%, transparent 55%, rgba(6,8,14,0.6) 100%)' }}
      />
      <div className="absolute left-1/2 top-14 z-10 -translate-x-1/2">
        <Button variant="secondary" size="sm" aria-label={t().butler.diagramDismiss} onClick={onDismiss}>
          <CloseIcon className="size-3.5" />
          {t().butler.diagramDismiss}
        </Button>
      </div>
      <div className="relative flex min-h-full flex-col items-center justify-center gap-7 px-6 py-24">
        {/* Fixed light text — the backdrop is dark in both app themes. */}
        <h2 className="max-w-[80vw] text-center text-2xl font-semibold text-neutral-100">{spec.title}</h2>
        <div
          ref={hostRef}
          className="w-full max-w-[82vw] [&_svg]:mx-auto [&_svg]:h-auto [&_svg]:max-h-[62vh] [&_svg]:max-w-full"
          dangerouslySetInnerHTML={{ __html: svg }}
        />
      </div>
    </div>
  );
}
