'use client';
// Dynamic content-window registry — the windows Butler "hands" the user mid-
// call (images, videos, links, documents), parallel to useWindows' three
// fixed panels but with no persisted "closed but remembered" slot: entries
// are created on demand and dropped from state on close. openContentWindow
// (domain/content.ts) is the pure dedupe-by-id rule; this hook just stages
// and dispatches into React state.
import { useCallback, useState } from 'react';
import { openContentWindow, type ContentItem, type ContentWindowState } from '@/features/butler/domain/content';

export interface ContentWindowsApi {
  readonly windows: readonly ContentWindowState[];
  readonly openContent: (item: ContentItem) => string;
  readonly closeContent: (id: string) => void;
  readonly closeAllContent: () => void;
  readonly focusContent: (id: string) => void;
  readonly moveContent: (id: string, x: number, y: number) => void;
  readonly resizeContent: (id: string, width: number, height: number) => void;
}

// Cycle the stagger offset so a burst of content windows doesn't march off
// the right edge of the screen.
const STAGGER_SLOTS = 6;

export function useContentWindows(): ContentWindowsApi {
  const [windows, setWindows] = useState<readonly ContentWindowState[]>([]);

  const openContent = useCallback((item: ContentItem): string => {
    setWindows((ws) => {
      const n = ws.length % STAGGER_SLOTS;
      return openContentWindow(ws, item, { x: 48 + n * 24, y: 96 + n * 32 });
    });
    return item.id;
  }, []);

  const closeContent = useCallback((id: string) => {
    setWindows((ws) => ws.filter((w) => w.item.id !== id));
  }, []);

  const closeAllContent = useCallback(() => {
    setWindows((ws) => (ws.length === 0 ? ws : []));
  }, []);

  const focusContent = useCallback((id: string) => {
    setWindows((ws) => {
      const top = ws.reduce((m, w) => Math.max(m, w.z), 0);
      return ws.map((w) => (w.item.id === id && w.z !== top ? { ...w, z: top + 1 } : w));
    });
  }, []);

  const moveContent = useCallback((id: string, x: number, y: number) => {
    setWindows((ws) => ws.map((w) => (w.item.id === id ? { ...w, x, y } : w)));
  }, []);

  const resizeContent = useCallback((id: string, width: number, height: number) => {
    setWindows((ws) => ws.map((w) => (w.item.id === id ? { ...w, width, height } : w)));
  }, []);

  return { windows, openContent, closeContent, closeAllContent, focusContent, moveContent, resizeContent };
}
