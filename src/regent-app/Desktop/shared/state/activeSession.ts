'use client';
// Which session the main pane is showing — published by ChatView (it owns the
// live/resumed id), read by the titlebar's session menu. Undefined = a fresh
// "New Conversation" (or a non-chat page).
import { type Store, createStore, useStore } from '@/shared/state/store';

const store: Store<{ id: string | undefined }> = createStore<{ id: string | undefined }>({
  id: undefined,
});

export function setActiveSession(id: string | undefined): void {
  store.setState({ id });
}

export function useActiveSession(): string | undefined {
  return useStore(store, (s) => s.id);
}
