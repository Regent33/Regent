'use client';
// The `/`-completion popup — a small list of matching `commands.list` rows
// anchored above the composer. Purely controlled: Composer owns the filtered
// items + selected index and wires ↑↓/Enter/Tab/Esc; this just renders.
import { t } from '@/shared/i18n/t';
import type { SlashCommand } from '@/features/chat/viewmodels/useSlashCommands';

export function SlashMenu({
  items,
  selected,
  onPick,
}: {
  items: readonly SlashCommand[];
  selected: number;
  onPick: (name: string) => void;
}) {
  const s = t().chat.composer;
  if (items.length === 0) return null;

  return (
    <div
      role="listbox"
      aria-label={s.slashCommands}
      className="absolute bottom-full left-6 right-6 z-20 mb-2 max-h-64 overflow-y-auto rounded-lg border border-stroke-secondary bg-surface p-1 motion-safe:animate-[fadeIn_120ms_ease-out]"
      style={{ boxShadow: 'var(--shadow-elev)' }}
    >
      {items.map((c, i) => (
        <div
          key={c.name}
          role="option"
          aria-selected={i === selected}
          // mousedown (not click) fires before the textarea's blur, so the
          // pick lands before focus would otherwise leave the composer.
          onMouseDown={(e) => {
            e.preventDefault();
            onPick(c.name);
          }}
          className={`cursor-pointer rounded-[4px] px-2.5 py-1.5 transition-colors ${
            i === selected ? 'bg-hover text-text-primary' : 'text-text-secondary'
          }`}
        >
          <p className="truncate font-mono text-[13px]">/{c.name}</p>
          {c.description !== '' && <p className="truncate text-xs text-text-tertiary">{c.description}</p>}
        </div>
      ))}
    </div>
  );
}
