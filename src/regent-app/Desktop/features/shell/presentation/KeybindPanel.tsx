'use client';
// "?" overlay: a static reference list of the shell's keyboard shortcuts.
// Shell-local state (see Shell.tsx), not the overlays store — it can float
// above whatever else is on screen without needing a new OverlayId. Scrim +
// click-away close it, mirroring CommandPalette's chrome; Esc is handled by
// the same keydown listener that opens it (Shell.tsx).
import { t } from '@/shared/i18n/t';
import { KEYBINDS } from '@/shared/state/keybinds';

export function KeybindPanel({ onClose }: { onClose: () => void }) {
  const s = t().shell.keybinds;

  return (
    <div className="fixed inset-0 z-[60]">
      <div className="absolute inset-0 bg-scrim" onClick={onClose} aria-hidden />
      <div
        role="dialog"
        aria-modal="true"
        aria-label={s.label}
        className="relative mx-auto mt-[20vh] w-[360px] max-w-[90vw] rounded-md border border-stroke-secondary bg-surface p-4 motion-safe:animate-[fadeIn_100ms_ease-out]"
        style={{ boxShadow: 'var(--shadow-elev)' }}
      >
        <p className="mb-3 text-sm font-semibold text-text-primary">{s.title}</p>
        <ul className="flex flex-col gap-2">
          {KEYBINDS.map((bind) => (
            <li key={bind.action} className="flex items-center justify-between gap-4 text-sm">
              <span className="text-text-secondary">{s.actions[bind.action]}</span>
              <kbd className="shrink-0 rounded border border-stroke-tertiary bg-hover px-1.5 py-0.5 text-[11px] text-text-tertiary">
                {bind.combo}
              </kbd>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
