'use client';
// Theme choice — 'light' | 'dark' | 'system' — on the same tiny external store
// as everything else. The pick persists to localStorage and is applied by
// stamping `data-theme` on <html>: 'light'/'dark' set the attribute (which
// wins over the OS media default in globals.css), 'system' removes it so the
// `prefers-color-scheme` media query drives. SSR-safe: the server snapshot is
// the environment-independent 'system', and every window/localStorage touch is
// guarded, so a static export hydrates without a mismatch. The no-flash inline
// script in app/layout.tsx stamps the attribute before first paint; init()
// here re-reads the same key so the store and the DOM start in agreement.
import { type Store, createStore, useStore } from '@/shared/state/store';

export type ThemeMode = 'light' | 'dark' | 'system';

const KEY = 'regent.theme';

interface ThemeState {
  readonly mode: ThemeMode;
}

const store: Store<ThemeState> = createStore<ThemeState>({ mode: 'system' });

function isMode(value: string | null): value is ThemeMode {
  return value === 'light' || value === 'dark' || value === 'system';
}

/** Reflect the mode onto <html>: explicit attribute for light/dark, none for
 * system (so the media default takes over). No-op off the browser. */
function apply(mode: ThemeMode): void {
  if (typeof document === 'undefined') return;
  const root = document.documentElement;
  if (mode === 'system') root.removeAttribute('data-theme');
  else root.setAttribute('data-theme', mode);
}

/** Read the persisted choice into the store (call once, client-side). The DOM
 * is already stamped by the inline script — this just aligns the store. */
export function initTheme(): void {
  if (typeof window === 'undefined') return;
  let stored: string | null = null;
  try {
    stored = window.localStorage.getItem(KEY);
  } catch {
    stored = null;
  }
  const mode = isMode(stored) ? stored : 'system';
  store.setState({ mode });
  apply(mode);
}

export function setMode(mode: ThemeMode): void {
  store.setState({ mode });
  apply(mode);
  try {
    window.localStorage.setItem(KEY, mode);
  } catch {
    /* private mode / storage disabled — the in-memory choice still holds */
  }
}

export function useTheme(): { mode: ThemeMode; setMode: (mode: ThemeMode) => void } {
  const mode = useStore(store, (s) => s.mode);
  return { mode, setMode };
}
