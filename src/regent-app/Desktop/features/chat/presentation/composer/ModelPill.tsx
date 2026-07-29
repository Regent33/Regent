'use client';
// Model hot-swap pill — sits left of send. Shows the active model's short
// name; click opens a popover of `model.list` rows, picking one fires
// `model.set` and shows the response's `note` as a transient hint.
import { useEffect, useRef, useState } from 'react';
import { deaconRequest } from '@/shared/infrastructure/rpc/client';
import { useActiveModel, useFallbackModel } from '@/shared/state/deaconBus';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { ListRow } from '@/shared/ui/ListRow';
import { ChevronDownIcon } from '@/shared/ui/icons';

interface ModelRow {
  readonly id: string;
  readonly display_name: string;
  readonly current: boolean;
}

const HINT_MS = 3200;

/** "claude-sonnet-4-6" → "sonnet-4-6"; "openai/gpt-5" → "gpt-5" — the part
 * after the last provider slash, or the bare id when there's none. */
function shortLabel(id: string): string {
  const tail = id.includes('/') ? (id.split('/').pop() ?? id) : id;
  return tail.length > 20 ? `${tail.slice(0, 18)}…` : tail;
}

function menuLabels(id: string): { label: string; provider?: string } {
  const parts = id.split('/');
  if (parts.length === 1) return { label: id };
  return { label: parts.at(-1) ?? id, provider: parts.slice(0, -1).join('/') };
}

export function ModelPill({ disabled = false }: { disabled?: boolean }) {
  const s = t().chat.composer;
  const [probed, setProbed] = useState('');
  // Live `model.changed` events (model.set anywhere, or a new primary applied
  // on the Model page) beat the mount-time probe.
  const current = useActiveModel() ?? probed;
  // Runtime failover (primary erroring, chain answering elsewhere) — shown as
  // a warning on the pill without touching the user's selected model.
  const fallback = useFallbackModel();
  const [models, setModels] = useState<readonly ModelRow[]>([]);
  const [query, setQuery] = useState('');
  const [open, setOpen] = useState(false);
  const [hint, setHint] = useState<string | undefined>(undefined);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void deaconRequest<{ model?: string }>('model.get', {}).then((r) => {
      if (r.ok && typeof r.value?.model === 'string') setProbed(r.value.model);
    });
  }, []);

  useEffect(() => {
    if (!open) return;
    void deaconRequest<ModelRow[]>('model.list', {}).then((r) => {
      if (r.ok && Array.isArray(r.value)) setModels(r.value);
    });
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onClick = (e: MouseEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    document.addEventListener('mousedown', onClick);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onClick);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);

  useEffect(() => {
    if (hint === undefined) return;
    const id = setTimeout(() => setHint(undefined), HINT_MS);
    return () => clearTimeout(id);
  }, [hint]);

  const pick = (modelId: string) => {
    setOpen(false);
    void deaconRequest<{ model?: string; note?: string }>('model.set', { model: modelId }).then((r) => {
      if (!r.ok) return;
      // `model.changed` from the deacon updates the label via useActiveModel;
      // the probe fallback is refreshed too for the pre-first-event case.
      if (typeof r.value?.model === 'string') setProbed(r.value.model);
      if (typeof r.value?.note === 'string') setHint(r.value.note);
    });
  };
  const visibleModels = models.filter((model) =>
    `${model.id} ${model.display_name}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()),
  );

  return (
    // The one shrinkable control on the bar: in a narrow composer the model
    // name gives up width (it already truncates) so Send never gets pushed
    // out. Everything else beside it is shrink-0.
    <div className="relative min-w-0 shrink" ref={rootRef}>
      <Button
        variant="ghost"
        size="sm"
        disabled={disabled}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={s.openModelPicker}
        onClick={() => {
          setQuery('');
          setOpen((v) => !v);
        }}
        title={fallback !== undefined ? `${s.fallbackActive} ${fallback}` : undefined}
      >
        {fallback !== undefined && (
          <span aria-hidden className="size-1.5 shrink-0 rounded-full bg-amber-500" />
        )}
        <span className={`max-w-28 truncate ${fallback !== undefined ? 'text-amber-500' : ''}`}>
          {fallback !== undefined ? shortLabel(fallback) : current !== '' ? shortLabel(current) : s.model}
        </span>
        <ChevronDownIcon className="size-3 shrink-0" />
      </Button>

      {open && (
        <div
          className="absolute bottom-full right-0 z-20 mb-2 w-96 max-w-[calc(100vw-3rem)] rounded-lg border border-stroke-secondary bg-surface motion-safe:animate-[fadeIn_120ms_ease-out]"
          style={{ boxShadow: 'var(--shadow-elev)' }}
        >
          <div className="border-b border-stroke-tertiary p-2">
            <input
              autoFocus
              aria-label={s.searchModels}
              value={query}
              placeholder={s.searchModels}
              className="w-full rounded-md border border-stroke-secondary bg-bg px-2.5 py-1.5 text-sm text-text-primary outline-none placeholder:text-text-tertiary focus:border-accent"
              onChange={(event) => setQuery(event.target.value)}
            />
          </div>
          <div
            role="listbox"
            aria-label={s.openModelPicker}
            className="max-h-80 overflow-y-auto overscroll-contain p-1"
          >
            {models.length === 0 ? (
              <p className="px-2.5 py-1.5 text-xs text-text-tertiary">…</p>
            ) : visibleModels.length === 0 ? (
              <p className="px-2.5 py-1.5 text-xs text-text-tertiary">{s.noModels}</p>
            ) : (
              visibleModels.map((model) => {
                const labels = menuLabels(model.id);
                return (
                  <ListRow
                    key={model.id}
                    dense
                    label={labels.label}
                    description={labels.provider}
                    title={model.id}
                    active={model.current}
                    onClick={() => pick(model.id)}
                  />
                );
              })
            )}
          </div>
        </div>
      )}

      {hint !== undefined && (
        <p
          role="status"
          className="absolute bottom-full right-0 z-20 mb-2 w-max max-w-[16rem] truncate rounded-sm bg-hover px-2 py-1 text-[11px] text-text-secondary motion-safe:animate-[fadeIn_120ms_ease-out]"
        >
          {hint}
        </p>
      )}
    </div>
  );
}
