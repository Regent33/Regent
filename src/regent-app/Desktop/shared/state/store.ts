'use client';
// Tiny external store on React's useSyncExternalStore — no dependency, no
// context provider. Holds one immutable state object; setState shallow-merges a
// partial (or a fn of the previous state) and notifies subscribers. useStore
// subscribes a selector and is SSR-safe: the server snapshot reads the same
// (environment-independent) initial state so hydration never mismatches. Keep
// selectors returning primitives/stable refs — a fresh object each call would
// loop useSyncExternalStore.
import { useSyncExternalStore } from 'react';

export interface Store<T> {
  readonly getState: () => T;
  readonly setState: (patch: Partial<T> | ((prev: T) => Partial<T>)) => void;
  readonly subscribe: (listener: () => void) => () => void;
}

export function createStore<T extends object>(initial: T): Store<T> {
  let state = initial;
  const listeners = new Set<() => void>();
  return {
    getState: () => state,
    setState: (patch) => {
      const next = typeof patch === 'function' ? patch(state) : patch;
      state = { ...state, ...next };
      for (const listener of listeners) listener();
    },
    subscribe: (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}

export function useStore<T extends object, S>(store: Store<T>, selector: (s: T) => S): S {
  const snapshot = () => selector(store.getState());
  return useSyncExternalStore(store.subscribe, snapshot, snapshot);
}
