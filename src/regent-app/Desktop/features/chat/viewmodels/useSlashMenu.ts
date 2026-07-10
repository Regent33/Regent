// The `/`-command menu controller — shared behavior wrapping useSlashCommands:
// detects a leading, otherwise-empty `/query`, filters the catalog, tracks the
// selected row, and handles ↑/↓/Enter/Tab/Esc. Composer and CodeView's task
// textarea both drive their input through this so the two surfaces can't drift.
import { useEffect, useMemo, useState, type KeyboardEvent } from 'react';
import { useSlashCommands, type SlashCommand } from '@/features/chat/viewmodels/useSlashCommands';

const SLASH_MATCH = /^\/(\S*)$/;

export interface SlashMenuController {
  readonly open: boolean;
  readonly items: readonly SlashCommand[];
  readonly selected: number;
  readonly accept: (name: string) => void;
  /** Returns true when the key was consumed (caller should stop handling it). */
  readonly onKeyDown: (e: KeyboardEvent<HTMLTextAreaElement>) => boolean;
  /** Clears an Esc-dismissal so the menu can reopen for the current value. */
  readonly reset: () => void;
}

export function useSlashMenu(
  value: string,
  setValue: (next: string) => void,
  focus: () => void,
): SlashMenuController {
  const [dismissedValue, setDismissedValue] = useState<string | undefined>(undefined);
  const [selected, setSelected] = useState(0);

  const slashMatch = SLASH_MATCH.exec(value);
  const open = slashMatch !== null && value !== dismissedValue;
  const query = (slashMatch?.[1] ?? '').toLowerCase();
  const commands = useSlashCommands(slashMatch !== null);
  // No cap here — the full `commands.list` catalog (~30 entries) is small
  // enough to render in one go; SlashMenu's own max-h + overflow-y-auto
  // handles the scrolling, so truncating here would just hide commands.
  const items = useMemo(
    () => commands.filter((c) => c.name.toLowerCase().startsWith(query)),
    [commands, query],
  );

  useEffect(() => setSelected(0), [query, open]);

  const accept = (name: string) => {
    setValue(`/${name} `);
    focus();
  };

  const reset = () => setDismissedValue(undefined);

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>): boolean => {
    if (!open || items.length === 0) return false;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSelected((i) => (i + 1) % items.length);
      return true;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSelected((i) => (i - 1 + items.length) % items.length);
      return true;
    }
    if (e.key === 'Enter' || e.key === 'Tab') {
      e.preventDefault();
      accept(items[selected].name);
      return true;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      setDismissedValue(value);
      return true;
    }
    return false;
  };

  return { open, items, selected, accept, onKeyDown, reset };
}
