'use client';
// Overlay routing without routes: which route-replacing surface (Settings,
// Skills) or the command palette floats above the shell. Only one is open at a
// time; the chat page stays mounted behind it. Esc closes via a single
// window-level listener (useOverlayEsc, mounted once in the shell).
import { useEffect } from 'react';
import { type Store, createStore, useStore } from '@/shared/state/store';

export type OverlayId = 'settings' | 'skills' | 'palette';

interface OverlayState {
  readonly current: OverlayId | null;
}

const store: Store<OverlayState> = createStore<OverlayState>({ current: null });

export function open(id: OverlayId): void {
  store.setState({ current: id });
}

export function close(): void {
  store.setState({ current: null });
}

/** ⌘K-style toggle: open `id`, or close if it is already the current overlay. */
export function toggle(id: OverlayId): void {
  store.setState((s) => ({ current: s.current === id ? null : id }));
}

export function useCurrentOverlay(): OverlayId | null {
  return useStore(store, (s) => s.current);
}

/** The one window-level Esc listener — closes whatever overlay is open. */
export function useOverlayEsc(): void {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && store.getState().current !== null) {
        e.preventDefault();
        close();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);
}
