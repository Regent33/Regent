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

  return (
    <div className="relative" ref={rootRef}>
      <Button
        variant="ghost"
        size="sm"
        disabled={disabled}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={s.openModelPicker}
        onClick={() => setOpen((v) => !v)}
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
          role="listbox"
          aria-label={s.openModelPicker}
          className="absolute bottom-full right-0 z-20 mb-2 w-56 rounded-lg border border-stroke-secondary bg-surface p-1 motion-safe:animate-[fadeIn_120ms_ease-out]"
          style={{ boxShadow: 'var(--shadow-elev)' }}
        >
          {models.length === 0 ? (
            <p className="px-2.5 py-1.5 text-xs text-text-tertiary">…</p>
          ) : (
            models.map((m) => (
              <ListRow key={m.id} dense label={m.display_name} active={m.current} onClick={() => pick(m.id)} />
            ))
          )}
        </div>
      )}

      {hint !== undefined && (
        <p
          role="status"
          className="absolute bottom-full right-0 z-20 mb-2 w-max max-w-[16rem] truncate rounded-[4px] bg-hover px-2 py-1 text-[11px] text-text-secondary motion-safe:animate-[fadeIn_120ms_ease-out]"
        >
          {hint}
        </p>
      )}
    </div>
  );
}
