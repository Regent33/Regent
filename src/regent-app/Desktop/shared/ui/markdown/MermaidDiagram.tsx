'use client';
// Fenced ```mermaid blocks — rendered to inline SVG entirely client-side (no
// CSP change, no consent gate: nothing leaves the app). Tall diagrams collapse
// behind ExpandableBlock like a long CodeBlock does; a parse error (common
// mid-stream, since a partial fence is invalid mermaid) falls back to the raw
// source with the error message underneath.
import { useEffect, useState } from 'react';
import { t } from '@/shared/i18n/t';
import { DiagramLightbox } from '@/shared/ui/markdown/DiagramLightbox';
import { ExpandableBlock } from '@/shared/ui/markdown/ExpandableBlock';
import { renderMermaid } from '@/shared/ui/markdown/mermaidLoader';

function RawSource({ code, error }: { code: string; error?: string }) {
  const s = t().chat.markdown;
  return (
    <div className="my-2 overflow-hidden rounded-md bg-hover">
      <div className="px-3 py-1.5 font-mono text-[11px] uppercase tracking-[0.04em] text-text-tertiary">
        mermaid
      </div>
      {error !== undefined && (
        <p className="px-3 pb-1 text-xs text-danger">
          {s.diagramError}: {error}
        </p>
      )}
      <pre className="m-0 overflow-x-auto px-3 pb-3 font-mono text-xs leading-relaxed text-text-primary">{code}</pre>
    </div>
  );
}

export function MermaidDiagram({ code }: { code: string }) {
  const trimmed = code.replace(/\n+$/, '');
  const [svg, setSvg] = useState<string | undefined>(undefined);
  const [error, setError] = useState<string | undefined>(undefined);
  const [zoomed, setZoomed] = useState(false);

  useEffect(() => {
    let alive = true;
    setSvg(undefined);
    setError(undefined);
    renderMermaid(trimmed).then(
      (result) => {
        if (alive) setSvg(result);
      },
      (err: unknown) => {
        if (alive) setError(err instanceof Error ? err.message : String(err));
      },
    );
    return () => {
      alive = false;
    };
  }, [trimmed]);

  if (error !== undefined) return <RawSource code={trimmed} error={error} />;
  if (svg === undefined) return <RawSource code={trimmed} />;

  return (
    <div className="my-2 overflow-hidden rounded-md bg-hover p-3">
      <ExpandableBlock>
        <button
          type="button"
          aria-label={t().chat.markdown.openDiagram}
          onClick={() => setZoomed(true)}
          className="block w-full cursor-zoom-in [&_svg]:mx-auto [&_svg]:h-auto [&_svg]:max-w-full"
          dangerouslySetInnerHTML={{ __html: svg }}
        />
      </ExpandableBlock>
      {zoomed && <DiagramLightbox code={trimmed} onClose={() => setZoomed(false)} />}
    </div>
  );
}
