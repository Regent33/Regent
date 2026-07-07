'use client';
// Overlay chrome — Hermes overlay-view fidelity: a blurred, dimmed scrim with
// a near-fullscreen inset card (rounded, bordered, elevated), a top-right
// close (×), and the shared fadeIn entrance. Clicks inside the card never
// reach the scrim's dismiss handler; Esc is handled by the overlays store.
import type { ReactNode } from 'react';
import { CloseIcon } from '@/shared/ui/icons';

export function Overlay({
  label,
  closeLabel,
  onClose,
  children,
}: {
  label: string;
  closeLabel: string;
  onClose: () => void;
  children: ReactNode;
}) {
  return (
    <div
      className="fixed inset-0 z-50 bg-scrim p-3 backdrop-blur-[2px] sm:p-6"
      role="presentation"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={label}
        className="relative flex h-full min-h-0 flex-col overflow-hidden rounded-xl border border-stroke-secondary bg-surface motion-safe:animate-[fadeIn_120ms_ease-out]"
        style={{ boxShadow: 'var(--shadow-elev)' }}
      >
        <button
          type="button"
          aria-label={closeLabel}
          className="absolute right-2.5 top-2.5 z-10 rounded-[6px] p-1.5 text-text-tertiary transition-colors hover:bg-hover hover:text-text-primary"
          onClick={onClose}
        >
          <CloseIcon className="size-4" />
        </button>
        <div className="min-h-0 flex-1 overflow-hidden">{children}</div>
      </div>
    </div>
  );
}
