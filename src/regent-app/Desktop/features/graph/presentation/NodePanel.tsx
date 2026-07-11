'use client';
// Right-side detail panel for the selected node: name, a colour-coded kind
// pill, a content preview, and its neighbours as clickable chips. Selecting a
// chip re-selects that node (which the page turns into a URL change + camera
// focus); the close button clears the selection.
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { CloseIcon } from '@/shared/ui/icons';
import { kindColor, type GraphNode } from '@/features/graph/viewmodels/useGraphData';

export interface Neighbor {
  readonly id: string;
  readonly name: string;
  readonly kind: string;
}

interface Props {
  node: GraphNode;
  neighbors: readonly Neighbor[];
  onSelect: (id: string) => void;
  onClose: () => void;
}

export function NodePanel({ node, neighbors, onSelect, onClose }: Props) {
  const s = t().graph.panel;
  const color = kindColor(node.kind);

  return (
    <aside
      className="absolute inset-y-0 right-0 z-10 flex w-[320px] max-w-[85%] flex-col gap-4 overflow-y-auto border-l border-stroke-secondary bg-surface p-4 motion-safe:animate-[fadeIn_140ms_ease-out]"
      style={{ boxShadow: 'var(--shadow-elev)' }}
    >
      <div className="flex items-start justify-between gap-2">
        <h2 className="min-w-0 break-words text-base font-semibold text-text-primary">{node.name}</h2>
        <Button variant="ghost" size="iconSm" aria-label={s.close} title={s.close} onClick={onClose}>
          <CloseIcon />
        </Button>
      </div>

      <div>
        <p className="mb-1 text-xs font-medium text-text-tertiary">{s.kind}</p>
        <span
          className="inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs font-medium"
          style={{ backgroundColor: `${color}2e`, color }}
        >
          <span className="size-2 rounded-full" style={{ backgroundColor: color }} />
          {node.kind}
        </span>
      </div>

      <div>
        <p className="mb-1 text-xs font-medium text-text-tertiary">{s.preview}</p>
        <p className="whitespace-pre-wrap break-words text-sm leading-relaxed text-text-secondary">
          {node.content.trim() !== '' ? node.content : s.noContent}
        </p>
      </div>

      <div>
        <p className="mb-1.5 text-xs font-medium text-text-tertiary">
          {s.linked} · {neighbors.length}
        </p>
        {neighbors.length === 0 ? (
          <p className="text-sm text-text-tertiary">{s.noLinks}</p>
        ) : (
          <div className="flex flex-wrap gap-1.5">
            {neighbors.map((nb) => (
              <button
                key={nb.id}
                type="button"
                onClick={() => onSelect(nb.id)}
                className="inline-flex items-center gap-1.5 rounded-full bg-hover px-2.5 py-1 text-xs text-text-secondary transition-colors hover:bg-stroke-secondary hover:text-text-primary"
              >
                <span className="size-2 shrink-0 rounded-full" style={{ backgroundColor: kindColor(nb.kind) }} />
                <span className="max-w-[12rem] truncate">{nb.name}</span>
              </button>
            ))}
          </div>
        )}
      </div>
    </aside>
  );
}
