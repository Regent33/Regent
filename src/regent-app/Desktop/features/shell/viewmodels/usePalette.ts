'use client';
// Command-palette state: ⌘K/Ctrl+K toggles, Esc closes, focus returns to the
// element that had it. The open/close bit lives in the overlays store (so the
// palette is one overlay among Settings/Skills); this hook owns the query,
// selection, and focus restore. Settings/Skills actions open overlays; the rest
// route.
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useRouter } from '@/shared/infrastructure/router/adapter';
import { t } from '@/shared/i18n/t';
import { close as closeOverlay, open as openOverlay, toggle as toggleOverlay, useCurrentOverlay } from '@/shared/state/overlays';

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
  const open = useCurrentOverlay() === 'palette';
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState(0);
  const restoreFocus = useRef<HTMLElement | null>(null);
  const wasOpen = useRef(false);

  const close = useCallback(() => closeOverlay(), []);

  const actions = useMemo<PaletteAction[]>(() => {
    const s = t().shell.palette.actions;
    const stub = (id: string, label: string, run: () => void): PaletteAction => ({ id, label, run });
    return [
      stub('home', s.home, () => router.push('/')),
      stub('new-session', s.newSession, () => router.push('/')),
      stub('skills', s.skills, () => openOverlay('skills')),
      stub('messaging', s.messaging, () => router.push('/messaging')),
      stub('artifacts', s.artifacts, () => router.push('/artifacts')),
      stub('cron', s.cron, () => router.push('/cron')),
      stub('profiles', s.profiles, () => router.push('/profiles')),
      stub('settings', s.settings, () => openOverlay('settings')),
    ];
  }, [router]);

  const filtered = useMemo(
    () => actions.filter((a) => a.label.toLowerCase().includes(query.toLowerCase())),
    [actions, query],
  );

  // ⌘K/Ctrl+K toggles the palette overlay from anywhere.
  useEffect(() => {
    const onGlobalKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        toggleOverlay('palette');
      }
    };
    window.addEventListener('keydown', onGlobalKey);
    return () => window.removeEventListener('keydown', onGlobalKey);
  }, []);

  // Focus + input reset ride the open bit, so every close path (Esc, scrim,
  // ⌘K, running an action) restores focus and clears the query consistently.
  useEffect(() => {
    if (open && !wasOpen.current) {
      restoreFocus.current = document.activeElement as HTMLElement | null;
    } else if (!open && wasOpen.current) {
      setQuery('');
      setSelected(0);
      restoreFocus.current?.focus();
      restoreFocus.current = null;
    }
    wasOpen.current = open;
  }, [open]);

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
