'use client';
// The Terminal tab: xterm.js in the webview, a real shell in the deacon.
//
// xterm.js rather than a hand-rolled emulator — ANSI parsing, scrollback,
// selection, and reflow are a project, not a component. The webview has no
// process access at all (see src-tauri/capabilities/default.json), so every byte
// travels `pty.write` / `pty.data` over the deacon bridge.
import { useEffect, useRef, useState } from 'react';
import { FitAddon } from '@xterm/addon-fit';
import { Terminal } from '@xterm/xterm';
import '@xterm/xterm/css/xterm.css';
import { t } from '@/shared/i18n/t';
import { deaconRequest } from '@/shared/infrastructure/rpc/client';
import { subscribe } from '@/shared/state/deaconBus';
import { useTheme } from '@/shared/state/theme';
import { decodeOutput, encodeInput } from '@/features/workspace/domain/ptyCodec';

/** Reads a CSS custom property so the terminal follows the app's palette
 * instead of shipping its own black. */
function token(name: string, fallback: string): string {
  if (typeof window === 'undefined') return fallback;
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return value === '' ? fallback : value;
}

export function TerminalTab({ sessionId }: { sessionId: string | undefined }) {
  const s = t().workspace.panel;
  const host = useRef<HTMLDivElement>(null);
  const [error, setError] = useState<string>();
  const { mode } = useTheme();

  // One effect owns the whole terminal lifecycle: xterm instance, pty, and the
  // subscription. Re-running it would orphan a shell, so the deps are the two
  // things that genuinely require a fresh terminal.
  useEffect(() => {
    const element = host.current;
    if (element === null) return;

    const dark =
      mode === 'dark' ||
      (mode === 'system' && window.matchMedia?.('(prefers-color-scheme: dark)').matches === true);

    const term = new Terminal({
      cursorBlink: true,
      fontSize: 12,
      // A stack, not one family: the first monospace font present wins, and
      // Consolas/Menlo/DejaVu covers Windows, macOS and Linux respectively.
      fontFamily: 'Consolas, Menlo, "DejaVu Sans Mono", monospace',
      theme: dark
        ? { background: token('--bg', '#1f1d1b'), foreground: token('--text-primary', '#eae6df') }
        : {
            background: token('--surface', '#f5f0e9'),
            foreground: token('--text-primary', '#2b2724'),
            // Light-mode cursor must be dark or it vanishes on a light surface.
            cursor: token('--text-primary', '#2b2724'),
          },
      // Bounded: the deacon batches output, but a `yes` still arrives, and an
      // unbounded buffer is a memory leak with a scrollbar.
      scrollback: 5000,
      allowProposedApi: true,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(element);
    fit.fit();

    let ptyId: string | undefined;
    let disposed = false;
    const unsubscribers: Array<() => void> = [];

    const start = async () => {
      const opened = await deaconRequest('pty.open', {
        session_id: sessionId,
        cols: term.cols,
        rows: term.rows,
      });
      if (!opened.ok) {
        setError(opened.error.message);
        return;
      }
      const id = (opened.value as { pty_id?: string }).pty_id;
      if (id === undefined) {
        setError(s.terminalFailed);
        return;
      }
      // The await above means the component may already be gone. Closing here
      // rather than leaking a shell nobody can see.
      if (disposed) {
        void deaconRequest('pty.close', { pty_id: id });
        return;
      }
      ptyId = id;

      unsubscribers.push(
        subscribe({ method: 'pty.data' }, (event) => {
          const params = event.params as { pty_id?: string; data?: string };
          if (params.pty_id !== id || typeof params.data !== 'string') return;
          // Bytes, not a string: xterm does its own incremental UTF-8 decoding,
          // which is what makes a character split across two messages render.
          term.write(decodeOutput(params.data));
        }),
        subscribe({ method: 'pty.exit' }, (event) => {
          if ((event.params as { pty_id?: string }).pty_id !== id) return;
          term.write(`\r\n\x1b[2m${s.terminalExited}\x1b[0m\r\n`);
        }),
      );

      // Keystrokes, paste, and control chords all arrive here.
      term.onData((data) => {
        void deaconRequest('pty.write', { pty_id: id, data: encodeInput(data) });
      });
      // Tell the shell its window size, so line editing and full-screen programs
      // wrap where the user can actually see.
      term.onResize(({ cols, rows }) => {
        void deaconRequest('pty.resize', { pty_id: id, cols, rows });
      });
    };
    void start();

    // The panel is drag-resizable, so the element changes size without the
    // window doing anything — a window listener alone would miss every drag.
    const observer =
      typeof ResizeObserver === 'undefined'
        ? undefined
        : new ResizeObserver(() => {
            // A zero-sized container (mid-layout, or the tab hidden) makes fit()
            // compute nonsense dimensions.
            if (element.clientWidth > 0 && element.clientHeight > 0) fit.fit();
          });
    observer?.observe(element);

    return () => {
      disposed = true;
      observer?.disconnect();
      for (const off of unsubscribers) off();
      if (ptyId !== undefined) void deaconRequest('pty.close', { pty_id: ptyId });
      term.dispose();
    };
  }, [sessionId, mode, s.terminalExited, s.terminalFailed]);

  return (
    <div className="flex h-full min-h-0 flex-col">
      {error !== undefined && (
        <p className="border-b border-stroke-tertiary px-2 py-1 text-[11px] text-danger">{error}</p>
      )}
      {/* h-full + the xterm CSS import is what gives the viewport its own
          scrollback; the panel body must not scroll it instead. */}
      <div ref={host} className="min-h-0 flex-1 overflow-hidden px-1 py-0.5" />
    </div>
  );
}
