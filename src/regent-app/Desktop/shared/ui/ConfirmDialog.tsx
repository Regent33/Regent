'use client';
// Small centered confirm popup — a scrim + compact card, unlike the
// near-fullscreen Overlay (that's for route-replacing surfaces; this is a
// transient yes/no for one destructive action). Esc and a scrim click both
// cancel; the default focus sits on Cancel so Enter never confirms by accident.
import { useEffect, useRef } from 'react';
import { Button } from '@/shared/ui/Button';

export function ConfirmDialog({
  title,
  description,
  confirmLabel,
  cancelLabel,
  onConfirm,
  onCancel,
}: {
  title: string;
  description?: string;
  confirmLabel: string;
  cancelLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const cardRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    cardRef.current?.querySelector<HTMLButtonElement>('[data-autofocus]')?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onCancel();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onCancel]);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-scrim p-4 backdrop-blur-[2px]"
      role="presentation"
      onClick={(e) => {
        if (e.target === e.currentTarget) onCancel();
      }}
    >
      <div
        ref={cardRef}
        role="alertdialog"
        aria-modal="true"
        aria-label={title}
        className="w-full max-w-sm rounded-xl border border-stroke-secondary bg-surface p-4 motion-safe:animate-[fadeIn_120ms_ease-out]"
        style={{ boxShadow: 'var(--shadow-elev)' }}
      >
        <p className="text-sm font-semibold text-text-primary">{title}</p>
        {description !== undefined && <p className="mt-1 text-xs text-text-secondary">{description}</p>}
        <div className="mt-4 flex justify-end gap-2">
          <Button data-autofocus variant="secondary" size="sm" onClick={onCancel}>
            {cancelLabel}
          </Button>
          <Button variant="danger" size="sm" onClick={onConfirm}>
            {confirmLabel}
          </Button>
        </div>
      </div>
    </div>
  );
}
