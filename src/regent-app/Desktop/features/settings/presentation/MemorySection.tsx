'use client';
// Memory & Context section — a Pending block (memory.pending, resolved via
// approve/reject) above the stored list (memory.list, pin/unpin toggle +
// a two-click "Forget" confirm owned locally by `confirmingId`).
import { useState } from 'react';
import { Button } from '@/shared/ui/Button';
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { EmptyState } from '@/shared/ui/EmptyState';
import { PinIcon } from '@/shared/ui/icons';
import { t } from '@/shared/i18n/t';
import { Section } from '@/features/settings/presentation/primitives';
import { ConfigField } from '@/features/settings/presentation/ConfigField';
import { useConfig } from '@/features/settings/viewmodels/useConfig';
import { useMemoryList } from '@/features/settings/viewmodels/useMemoryList';
import { useMemoryPending } from '@/features/settings/viewmodels/useMemoryPending';

export function MemorySection() {
  const s = t().settings.memory;
  const cfg = useConfig();
  const list = useMemoryList();
  const pending = useMemoryPending();
  const [confirmingId, setConfirmingId] = useState<string>();

  const forget = (id: string) => {
    if (confirmingId === id) {
      list.forget(id);
      setConfirmingId(undefined);
    } else {
      setConfirmingId(id);
    }
  };

  return (
    <Section title={s.title}>
      <h3 className="text-sm font-semibold text-text-primary">{s.contextTitle}</h3>
      {cfg.loading && (
        <div className="mt-2">
          <Loader />
        </div>
      )}
      {cfg.error !== undefined && <ErrorState description={cfg.error} />}
      {!cfg.loading && cfg.error === undefined && (
        <>
          <ConfigField
            cfg={cfg}
            path="context.max_tokens"
            label={s.maxTokensLabel}
            description={s.maxTokensHint}
            applyLabel={s.apply}
            control={{ kind: 'number', min: 1, step: 1000 }}
          />
          <ConfigField
            cfg={cfg}
            path="context.trigger_fraction"
            label={s.triggerLabel}
            description={s.triggerHint}
            applyLabel={s.apply}
            control={{ kind: 'number', min: 0, max: 1, step: 0.05 }}
          />
          <ConfigField
            cfg={cfg}
            path="context.protect_last_n"
            label={s.protectLabel}
            description={s.protectHint}
            applyLabel={s.apply}
            control={{ kind: 'number', min: 0, step: 1 }}
          />
          <ConfigField
            cfg={cfg}
            path="limits.max_turn_tokens"
            label={s.maxTurnLabel}
            description={s.maxTurnHint}
            applyLabel={s.apply}
            control={{ kind: 'number', min: 1, step: 1000 }}
          />
          {cfg.writeError !== undefined && <ErrorState compact description={cfg.writeError} />}
          {cfg.note !== undefined && <p className="mt-2 text-xs text-text-tertiary">{cfg.note}</p>}
        </>
      )}

      {pending.pending.length > 0 && (
        <div className="mt-4">
          <h3 className="text-sm font-semibold text-text-primary">{s.pendingTitle}</h3>
          {pending.error !== undefined && <ErrorState compact description={pending.error} />}
          {pending.pending.map((w) => (
            <div key={w.id} className="mt-2 rounded-[6px] bg-hover px-3 py-2">
              <p className="text-sm text-text-primary">{w.name ?? w.kind ?? w.id}</p>
              {w.content !== undefined && <p className="mt-0.5 text-xs text-text-tertiary">{w.content}</p>}
              <div className="mt-2 flex gap-2">
                <Button size="sm" onClick={() => pending.approve(w.id)}>
                  {s.approve}
                </Button>
                <Button variant="secondary" size="sm" onClick={() => pending.reject(w.id)}>
                  {s.reject}
                </Button>
              </div>
            </div>
          ))}
        </div>
      )}

      <h3 className="mt-5 text-sm font-semibold text-text-primary">{s.listTitle}</h3>
      {list.loading && (
        <div className="mt-2">
          <Loader />
        </div>
      )}
      {list.error !== undefined && <ErrorState description={list.error} />}
      {!list.loading && list.error === undefined && list.nodes.length === 0 && (
        <EmptyState title={s.empty} />
      )}
      {list.nodes.map((node) => (
        <div key={node.id} className="mt-2 flex items-start gap-2.5 rounded-[6px] px-1 py-1.5">
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm text-text-primary">{node.name ?? node.id}</p>
            {node.content !== undefined && (
              <p className="truncate text-xs text-text-tertiary">{node.content}</p>
            )}
          </div>
          <Button
            variant={node.pinned ? 'default' : 'ghost'}
            size="iconSm"
            aria-label={node.pinned ? s.unpin : s.pin}
            onClick={() => list.togglePin(node)}
          >
            <PinIcon className="size-3.5" />
          </Button>
          <Button variant={confirmingId === node.id ? 'default' : 'ghost'} size="sm" onClick={() => forget(node.id)}>
            {confirmingId === node.id ? s.forgetConfirm : s.forget}
          </Button>
        </div>
      ))}
    </Section>
  );
}
