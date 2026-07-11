'use client';
// Butler Mode visibility — toggled from the composer's butler button. Lives in
// a shared store (like rail.ts) so the deeply nested composer can flip it while
// AppShell owns the mount: ButlerView exists only while open, so the mic and
// audio graph live exactly as long as the view.
import { type Store, createStore, useStore } from '@/shared/state/store';

const store: Store<{ open: boolean }> = createStore<{ open: boolean }>({ open: false });

export function toggleButler(): void {
  store.setState((prev) => ({ open: !prev.open }));
}

export function closeButler(): void {
  store.setState({ open: false });
}

export function useButlerOpen(): boolean {
  return useStore(store, (s) => s.open);
}
