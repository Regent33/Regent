'use client';
// Butler's floating-window registry: open/close, focus (z-order), and last
// position per window. Windows spawn staggered near the top-left cluster.
// ponytail: no snap-docking yet — free drag + remembered positions; add snap
// zones when a second window type makes the cluster real.
import { useCallback, useState } from 'react';

export interface ButlerWindow {
  readonly id: string;
  readonly x: number;
  readonly y: number;
  readonly z: number;
  readonly open: boolean;
}

export interface WindowsApi {
  readonly windows: readonly ButlerWindow[];
  readonly toggle: (id: string) => void;
  readonly focus: (id: string) => void;
  readonly move: (id: string, x: number, y: number) => void;
}

export function useWindows(ids: readonly string[]): WindowsApi {
  const [windows, setWindows] = useState<ButlerWindow[]>(() =>
    ids.map((id, i) => ({ id, x: 24 + i * 28, y: 84 + i * 40, z: i + 1, open: false })),
  );

  const focus = useCallback((id: string) => {
    setWindows((ws) => {
      const top = Math.max(...ws.map((w) => w.z));
      return ws.map((w) => (w.id === id && w.z !== top ? { ...w, z: top + 1 } : w));
    });
  }, []);

  const toggle = useCallback((id: string) => {
    setWindows((ws) => {
      const top = Math.max(...ws.map((w) => w.z));
      return ws.map((w) => (w.id === id ? { ...w, open: !w.open, z: top + 1 } : w));
    });
  }, []);

  const move = useCallback((id: string, x: number, y: number) => {
    setWindows((ws) => ws.map((w) => (w.id === id ? { ...w, x, y } : w)));
  }, []);

  return { windows, toggle, focus, move };
}
