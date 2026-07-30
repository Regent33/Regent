'use client';
// The `/`-completion popup — a small list of matching `commands.list` rows
// anchored above the composer. Purely controlled: Composer owns the filtered
// items + selected index and wires ↑↓/Enter/Tab/Esc; this just renders. The
// full catalog can exceed the max-h-64 viewport, so the highlighted row is
// scrolled into view as ↑/↓ move past it.
import { useEffect, useRef } from 'react';
import { CloseIcon } from '@/shared/ui/icons';
import { t } from '@/shared/i18n/t';
import type { SlashCommand } from '@/features/chat/viewmodels/useSlashCommands';

export function SlashMenu({
  items,
  selected,
  onPick,
  onClose,
}: {
  items: readonly SlashCommand[];
  selected: number;
  onPick: (name: string) => void;
  /** Dismiss the popup. Esc already did this; a pointer had no way to. */
  onClose: () => void;
}) {
  const s = t().chat.composer;
  const selectedRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    selectedRef.current?.scrollIntoView({ block: 'nearest' });
  }, [selected]);

  if (items.length === 0) return null;

  return (
    <div
      role="listbox"
      aria-label={s.slashCommands}
      className="absolute bottom-full left-6 right-6 z-20 mb-2 max-h-64 overflow-y-auto rounded-lg border border-stroke-secondary bg-surface p-1 motion-safe:animate-[fadeIn_120ms_ease-out]"
      style={{ boxShadow: 'var(--shadow-elev)' }}
    >
      {/* Esc dismisses, but nothing did with a pointer — the menu could only be
          closed by deleting the "/". Sticky so it stays reachable while the
          list scrolls. mousedown, like the rows, so it lands before the
          textarea's blur. */}
      <button
        type="button"
        aria-label={s.closeCommands}
        title={s.closeCommands}
        className="sticky top-0 z-10 float-right flex size-6 cursor-pointer items-center justify-center rounded-sm text-text-tertiary hover:bg-hover hover:text-text-primary"
        onMouseDown={(e) => {
          e.preventDefault();
          onClose();
        }}
      >
        <CloseIcon className="size-3.5" />
      </button>
      {items.map((c, i) => (
        <div
          key={c.name}
          ref={i === selected ? selectedRef : undefined}
          role="option"
          aria-selected={i === selected}
          // mousedown (not click) fires before the textarea's blur, so the
          // pick lands before focus would otherwise leave the composer.
          onMouseDown={(e) => {
            e.preventDefault();
            onPick(c.name);
          }}
          className={`cursor-pointer rounded-sm px-2.5 py-1.5 transition-colors ${
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
