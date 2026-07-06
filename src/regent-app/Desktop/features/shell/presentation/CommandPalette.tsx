'use client';
// ⌘K overlay — flat list floating on the elevation shadow + hairline
// (borderless-plus-shadow, never a hard border). Esc closes; focus returns
// to the trigger (handled in the viewmodel).
import { t } from '@/shared/i18n/t';
import type { PaletteState } from '@/features/shell/viewmodels/usePalette';

export function CommandPalette({ palette }: { palette: PaletteState }) {
  const s = t().shell.palette;
  if (!palette.open) return null;

  return (
    <div className="fixed inset-0 z-50">
      {/* Scrim — clicking it dismisses, same as Esc. */}
      <div className="absolute inset-0 bg-scrim" onClick={palette.close} aria-hidden />
      <div
        role="dialog"
        aria-modal="true"
        aria-label={s.label}
        className="relative mx-auto mt-[15vh] w-[560px] max-w-[90vw] rounded-md border border-stroke-secondary bg-surface motion-safe:animate-[fadeIn_100ms_ease-out]"
        style={{ boxShadow: 'var(--shadow-elev)' }}
        onKeyDown={palette.onKeyDown}
      >
        <input
          autoFocus
          value={palette.query}
          onChange={(e) => palette.setQuery(e.target.value)}
          placeholder={s.placeholder}
          aria-label={s.label}
          className="w-full border-b border-stroke-tertiary bg-transparent px-4 py-3 text-sm text-text-primary outline-none placeholder:text-text-tertiary focus-visible:outline-none"
        />
        <ul className="max-h-[300px] overflow-y-auto py-1" role="listbox" aria-label={s.label}>
          {palette.filtered.length === 0 && (
            <li className="px-4 py-2.5 text-sm text-text-tertiary">{s.empty}</li>
          )}
          {palette.filtered.map((action, i) => (
            <li key={action.id} role="option" aria-selected={i === palette.selected}>
              <button
                type="button"
                className={`w-full cursor-pointer px-4 py-2 text-left text-sm transition-colors duration-100 ${
                  i === palette.selected ? 'bg-hover text-text-primary' : 'text-text-secondary hover:bg-hover'
                }`}
                onClick={() => {
                  action.run();
                  palette.close();
                }}
              >
                {action.label}
              </button>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
