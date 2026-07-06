'use client';
// Command-palette state: ⌘K/Ctrl+K toggles, Esc closes, focus returns to the
// element that had it. Navigation stubs — only Home routes today; the rest
// close the palette until their pages land (M2+).
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useRouter } from 'next/navigation';
import { t } from '@/shared/i18n/t';

export interface PaletteAction {
  readonly id: string;
  readonly label: string;
  readonly run: () => void;
}

export interface PaletteState {
  readonly open: boolean;
  readonly query: string;
  readonly selected: number;
  readonly filtered: readonly PaletteAction[];
  readonly setQuery: (q: string) => void;
  readonly close: () => void;
  readonly onKeyDown: (e: React.KeyboardEvent) => void;
}

export function usePalette(): PaletteState {
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState(0);
  const restoreFocus = useRef<HTMLElement | null>(null);

  const close = useCallback(() => {
    setOpen(false);
    setQuery('');
    setSelected(0);
    restoreFocus.current?.focus();
    restoreFocus.current = null;
  }, []);

  const actions = useMemo<PaletteAction[]>(() => {
    const s = t().shell.palette.actions;
    const stub = (id: string, label: string, run: () => void): PaletteAction => ({ id, label, run });
    return [
      stub('home', s.home, () => router.push('/')),
      stub('new-session', s.newSession, () => router.push('/')),
      stub('code', s.code, () => router.push('/code')),
      stub('skills', s.skills, () => router.push('/skills')),
      stub('messaging', s.messaging, () => router.push('/messaging')),
      stub('artifacts', s.artifacts, () => router.push('/artifacts')),
      stub('cron', s.cron, () => router.push('/cron')),
      stub('profiles', s.profiles, () => router.push('/profiles')),
      stub('settings', s.settings, () => router.push('/settings')),
    ];
  }, [router]);

  const filtered = useMemo(
    () => actions.filter((a) => a.label.toLowerCase().includes(query.toLowerCase())),
    [actions, query],
  );

  useEffect(() => {
    const onGlobalKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        setOpen((was) => {
          if (!was) restoreFocus.current = document.activeElement as HTMLElement | null;
          else restoreFocus.current?.focus();
          return !was;
        });
        setQuery('');
        setSelected(0);
      }
    };
    window.addEventListener('keydown', onGlobalKey);
    return () => window.removeEventListener('keydown', onGlobalKey);
  }, []);

  const onKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        close();
      } else if (e.key === 'ArrowDown') {
        e.preventDefault();
        setSelected((i) => Math.min(i + 1, filtered.length - 1));
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setSelected((i) => Math.max(i - 1, 0));
      } else if (e.key === 'Enter') {
        e.preventDefault();
        const action = filtered[selected];
        if (action) {
          action.run();
          close();
        }
      }
    },
    [close, filtered, selected],
  );

  return { open, query, selected, filtered, setQuery, close, onKeyDown };
}
