'use client';
// A diagram spec (the ```json / ```present block the voice/butler surface emits)
// rendered as an actual diagram in the chat transcript, instead of raw JSON.
// Reuses the same spec→mermaid path the full-screen butler backdrop uses, drawn
// inline via MermaidDiagram (which owns its overflow — no bleed out of the row).
import { extractPresentSpec, type PresentSpec } from '@/shared/diagram/presentSpec';
import { specToMermaid } from '@/shared/diagram/diagramMermaid';
import { MermaidDiagram } from '@/shared/ui/markdown/MermaidDiagram';

/** A valid diagram spec from a fenced block's body, or null (→ render as code).
 * Only json/present/untagged blocks that are a single `{…"type"…}` object are
 * even tried; the strict validator rejects any other JSON so real code blocks
 * (config samples, API payloads) stay code blocks. */
export function specFromCode(language: string, code: string): PresentSpec | null {
  const lang = language.trim().toLowerCase();
  if (lang !== 'json' && lang !== 'present' && lang !== '') return null;
  const trimmed = code.trim();
  if (!trimmed.startsWith('{') || !trimmed.includes('"type"')) return null;
  return extractPresentSpec(trimmed).spec;
}

/** The diagram plus its title. `specToMermaid` deliberately emits no title —
 * six of the ten types cannot carry one — so the surface draws it, in the app's
 * own type rather than mermaid's per-diagram-type styling. */
export function SpecDiagram({ spec }: { spec: PresentSpec }) {
  const title = spec.title.trim();
  return (
    <figure className="my-2 overflow-hidden rounded-md border border-border-subtle bg-surface-raised">
      {title !== '' && (
        <figcaption className="border-b border-border-subtle px-3 py-1.5 text-xs font-semibold text-text-secondary">
          {title}
        </figcaption>
      )}
      <div className="px-2 py-1">
        <MermaidDiagram code={specToMermaid(spec)} />
      </div>
    </figure>
  );
}
