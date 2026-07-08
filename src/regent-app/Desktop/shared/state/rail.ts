'use client';
// Left-rail visibility — toggled from the titlebar's panel button; the shell
// animates the width so the rail slides rather than pops.
import { type Store, createStore, useStore } from '@/shared/state/store';

const store: Store<{ open: boolean }> = createStore<{ open: boolean }>({ open: true });

export function toggleRail(): void {
  store.setState((prev) => ({ open: !prev.open }));
}

export function useRailOpen(): boolean {
  return useStore(store, (s) => s.open);
}
