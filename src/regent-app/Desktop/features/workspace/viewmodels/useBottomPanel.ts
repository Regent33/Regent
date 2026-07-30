'use client';
// State for the workspace's bottom panel: open/closed, height, the space
// available to it, and the Ctrl/Cmd+` toggle. Extracted from WorkspacePanel,
// which was already past this repo's file ceiling before the panel arrived.
import { type RefObject, useCallback, useEffect, useState } from 'react';
import { PANEL_DEFAULT_HEIGHT } from '@/features/workspace/domain/panelModel';

/** `container` is the element the panel lives inside — its height is what the
 * panel's drag cap is measured against. */
export function useBottomPanel(container: RefObject<HTMLElement | null>) {
  // Closed by default: chat should look unchanged until the terminal is asked
  // for. Height survives open/close within the session.
  const [open, setOpen] = useState(false);
  const [height, setHeight] = useState(PANEL_DEFAULT_HEIGHT);
  const [available, setAvailable] = useState(0);

  const toggle = useCallback(() => setOpen((current) => !current), []);
  const close = useCallback(() => setOpen(false), []);

  // Measured, not derived from `window.innerHeight`: the titlebar, the panel
  // header and the git toolbar all sit outside this box, and guessing at their
  // heights is how a drag ends up squeezing the editor to nothing.
  useEffect(() => {
    const el = container.current;
    // ResizeObserver is in every browser this ships to, but guard anyway — a
    // missing observer must leave a usable panel, not a crashed render.
    if (el === null || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(([entry]) => {
      setAvailable(entry?.contentRect.height ?? 0);
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [container]);

  // Ctrl/Cmd+` — the chord people arrive with. Capture phase for the same
  // reason the save shortcut uses it: the webview has its own handling to beat.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== '`' || !(e.ctrlKey || e.metaKey)) return;
      e.preventDefault();
      setOpen((current) => !current);
    };
    window.addEventListener('keydown', onKey, { capture: true });
    return () => window.removeEventListener('keydown', onKey, { capture: true });
  }, []);

  return { open, toggle, close, height, setHeight, available };
}
