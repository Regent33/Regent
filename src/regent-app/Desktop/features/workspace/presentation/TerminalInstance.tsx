'use client';
// ONE terminal: an xterm instance in the webview bound to one pty in the deacon.
// The tab strip and the set of terminals live in TerminalTab.tsx.
//
// Stays MOUNTED when its tab is inactive (hidden with CSS, see `visible`).
// Unmounting would close the pty and kill whatever is running in it — switching
// tabs must not do that.
import { useEffect, useRef, useState } from 'react';
import { FitAddon } from '@xterm/addon-fit';
import { SearchAddon } from '@xterm/addon-search';
import { WebLinksAddon } from '@xterm/addon-web-links';
import { WebglAddon } from '@xterm/addon-webgl';
import { Terminal } from '@xterm/xterm';
import '@xterm/xterm/css/xterm.css';
import { t } from '@/shared/i18n/t';
import { deaconRequest } from '@/shared/infrastructure/rpc/client';
import { subscribe } from '@/shared/state/deaconBus';
import { useTheme } from '@/shared/state/theme';
import { decodeOutput, encodeInput } from '@/features/workspace/domain/ptyCodec';
import { isFollowClick, terminalAction } from '@/features/workspace/domain/terminalKeys';
import { openExternal } from '@/shared/infrastructure/opener';

function token(name: string, fallback: string): string {
  if (typeof window === 'undefined') return fallback;
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return value === '' ? fallback : value;
}

export interface TerminalInstanceProps {
  readonly sessionId: string | undefined;
  readonly visible: boolean;
  readonly onExit: () => void;
  readonly onNewTerminal: () => void;
}

export function TerminalInstance({
  sessionId,
  visible,
  onExit,
  onNewTerminal,
}: TerminalInstanceProps) {
  const s = t().workspace.panel;
  const host = useRef<HTMLDivElement>(null);
  const fitRef = useRef<FitAddon>(null);
  const [error, setError] = useState<string>();
  const [find, setFind] = useState<string>();
  const searchRef = useRef<SearchAddon>(null);
  const { mode } = useTheme();

  // Latest-callback refs: the terminal is built once, but the parent's handlers
  // are new closures on every render. Reading through a ref keeps the effect
  // from depending on them, which would tear down the shell on each render.
  const onExitRef = useRef(onExit);
  onExitRef.current = onExit;
  const onNewRef = useRef(onNewTerminal);
  onNewRef.current = onNewTerminal;

  useEffect(() => {
    const element = host.current;
    if (element === null) return;

    const dark =
      mode === 'dark' ||
      (mode === 'system' && window.matchMedia?.('(prefers-color-scheme: dark)').matches === true);

    const term = new Terminal({
      cursorBlink: true,
      fontSize: 12,
      fontFamily: 'Consolas, Menlo, "DejaVu Sans Mono", monospace',
      theme: dark
        ? { background: token('--bg', '#1f1d1b'), foreground: token('--text-primary', '#eae6df') }
        : {
            background: token('--surface', '#f5f0e9'),
            foreground: token('--text-primary', '#2b2724'),
            cursor: token('--text-primary', '#2b2724'),
          },
      scrollback: 5000,
      allowProposedApi: true,
    });
    const fit = new FitAddon();
    const search = new SearchAddon();
    term.loadAddon(fit);
    term.loadAddon(search);
    // Ctrl/Cmd+click a URL to open it in the system browser. The addon detects
    // links; `openExternal` is the app's existing seam for leaving the webview —
    // http(s) only, so a `file://` or `javascript:` in terminal output cannot
    // navigate the app.
    term.loadAddon(
      new WebLinksAddon((event, uri) => {
        if (!isFollowClick(event)) return;
        openExternal(uri);
      }),
    );
    term.open(element);
    // GPU renderer — xterm's DOM default is one node per cell. Must come after
    // open() (it needs a canvas) and must not be fatal without WebGL.
    try {
      term.loadAddon(new WebglAddon());
    } catch {
      // DOM renderer stays: slower, correct, not worth interrupting anyone.
    }
    fit.fit();
    fitRef.current = fit;
    searchRef.current = search;

    // Keyboard. Returning FALSE means "xterm must not handle this" — which is
    // how a chord gets intercepted before it reaches the shell.
    term.attachCustomKeyEventHandler((event) => {
      const action = terminalAction(event, term.hasSelection());
      if (action === 'shell') return true;
      if (event.type !== 'keydown') return false;
      switch (action) {
        case 'copy':
          void navigator.clipboard?.writeText(term.getSelection());
          break;
        case 'paste':
          void navigator.clipboard
            ?.readText()
            .then((text) => text !== '' && term.paste(text))
            // A denied clipboard read must not take the terminal down with it.
            .catch(() => undefined);
          break;
        case 'selectAll':
          term.selectAll();
          break;
        case 'clear':
          term.clear();
          break;
        case 'search':
          setFind((current) => current ?? '');
          break;
        case 'newTerminal':
          onNewRef.current();
          break;
      }
      return false;
    });

    let ptyId: string | undefined;
    let disposed = false;
    const offs: Array<() => void> = [];

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
      // Unmounted during the round trip — close rather than leak a shell.
      if (disposed) {
        void deaconRequest('pty.close', { pty_id: id });
        return;
      }
      ptyId = id;
      offs.push(
        subscribe({ method: 'pty.data' }, (event) => {
          const p = event.params as { pty_id?: string; data?: string };
          if (p.pty_id !== id || typeof p.data !== 'string') return;
          // Bytes: xterm decodes UTF-8 incrementally, so a character split
          // across two messages still renders.
          term.write(decodeOutput(p.data));
        }),
        subscribe({ method: 'pty.exit' }, (event) => {
          if ((event.params as { pty_id?: string }).pty_id !== id) return;
          term.write(`\r\n\x1b[2m${s.terminalExited}\x1b[0m\r\n`);
          onExitRef.current();
        }),
      );
      term.onData((data) => {
        void deaconRequest('pty.write', { pty_id: id, data: encodeInput(data) });
      });
      term.onResize(({ cols, rows }) => {
        void deaconRequest('pty.resize', { pty_id: id, cols, rows });
      });
    };
    void start();

    const observer =
      typeof ResizeObserver === 'undefined'
        ? undefined
        : new ResizeObserver(() => {
            if (element.clientWidth > 0 && element.clientHeight > 0) fit.fit();
          });
    observer?.observe(element);

    return () => {
      disposed = true;
      observer?.disconnect();
      for (const off of offs) off();
      if (ptyId !== undefined) void deaconRequest('pty.close', { pty_id: ptyId });
      term.dispose();
    };
  }, [sessionId, mode, s.terminalExited, s.terminalFailed]);

  // A hidden element has no size, so xterm's last fit was computed against 0×0.
  // Re-fit on the way back in or the terminal renders one column wide.
  useEffect(() => {
    if (!visible) return;
    const id = requestAnimationFrame(() => fitRef.current?.fit());
    return () => cancelAnimationFrame(id);
  }, [visible]);

  return (
    // `hidden` rather than unmounting: unmounting closes the pty and kills
    // whatever is running in it.
    <div className={`${visible ? 'flex' : 'hidden'} h-full min-h-0 flex-col`}>
      {error !== undefined && (
        <p className="border-b border-stroke-tertiary px-2 py-1 text-[11px] text-danger">{error}</p>
      )}
      {find !== undefined && (
        <div className="flex items-center gap-1.5 border-b border-stroke-tertiary px-2 py-1">
          {/* eslint-disable-next-line jsx-a11y/no-autofocus -- opened BY a chord;
              landing anywhere else would need a second click to type. */}
          <input
            autoFocus
            value={find}
            placeholder={s.searchPlaceholder}
            aria-label={s.searchPlaceholder}
            className="min-w-0 flex-1 bg-transparent text-[11px] outline-none"
            onChange={(e) => {
              setFind(e.target.value);
              searchRef.current?.findNext(e.target.value);
            }}
            onKeyDown={(e) => {
              if (e.key === 'Escape') setFind(undefined);
              if (e.key === 'Enter') {
                searchRef.current?.[e.shiftKey ? 'findPrevious' : 'findNext'](find);
              }
            }}
          />
          <button
            type="button"
            aria-label={s.searchClose}
            className="text-[11px] text-text-tertiary hover:text-text-primary"
            onClick={() => setFind(undefined)}
          >
            ×
          </button>
        </div>
      )}
      <div ref={host} className="min-h-0 flex-1 overflow-hidden px-3 py-2" />
    </div>
  );
}
