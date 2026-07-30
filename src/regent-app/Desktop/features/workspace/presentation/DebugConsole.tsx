'use client';
// The Debug Console: the JSON-RPC traffic between this window and the deacon.
//
//   ← method (12ms)        a request the app made, and its round trip
//   ← method  error…       one that failed, with the message
//   → method {…}           a notification the deacon pushed
//
// Direction arrows rather than words because the whole point is scanning: which
// side started this, and did it come back.
import { useEffect, useMemo, useState } from 'react';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { onRpcTraffic } from '@/shared/infrastructure/rpc/client';
import { subscribe } from '@/shared/state/deaconBus';
import { NO_LINES, type OutputLine, appendLines } from '@/features/workspace/domain/outputLines';
import { LogView } from '@/features/workspace/presentation/LogView';

/** Params are truncated, not pretty-printed: a `pty.data` payload or a base64
 * image would otherwise bury every other line in the console. */
const MAX_PARAMS = 160;

function summarize(params: Record<string, unknown>): string {
  // A circular or unserializable payload must not take the console down with
  // it — this is the surface people open when something is already wrong.
  let text: string;
  try {
    text = JSON.stringify(params) ?? '';
  } catch {
    return '(unserializable)';
  }
  return text.length <= MAX_PARAMS ? text : `${text.slice(0, MAX_PARAMS)}…`;
}

export function DebugConsole() {
  const s = t().workspace.panel;
  const [lines, setLines] = useState<readonly OutputLine[]>(NO_LINES);
  const [filter, setFilter] = useState('');

  useEffect(() => {
    const add = (text: string, tone: OutputLine['tone']) =>
      setLines((current) => appendLines(current, [text], tone));

    const offTraffic = onRpcTraffic((traffic) => {
      const head = `← ${traffic.method} (${traffic.ms}ms)`;
      add(traffic.ok ? head : `${head} ${traffic.detail ?? 'failed'}`, traffic.ok ? 'normal' : 'error');
    });
    // `pty.data` is excluded by hand. It is one notification per 16ms per open
    // terminal, so leaving it in makes the console unreadable and unscrollable
    // the moment a terminal is running — and its contents are already on screen
    // in the terminal itself.
    const offEvents = subscribe({}, (event) => {
      if (event.method === 'pty.data') return;
      add(`→ ${event.method} ${summarize(event.params)}`, 'muted');
    });
    return () => {
      offTraffic();
      offEvents();
    };
  }, []);

  const needle = filter.trim().toLowerCase();
  const shown = useMemo(
    () => (needle === '' ? lines : lines.filter((l) => l.text.toLowerCase().includes(needle))),
    [lines, needle],
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-2 border-b border-stroke-tertiary px-2 py-1">
        <input
          value={filter}
          placeholder={s.filterPlaceholder}
          aria-label={s.filterPlaceholder}
          className="min-w-0 flex-1 bg-transparent text-[11px] text-text-primary outline-none"
          onChange={(e) => setFilter(e.target.value)}
        />
        <Button size="sm" variant="ghost" onClick={() => setLines(NO_LINES)}>
          {s.clear}
        </Button>
      </div>
      <LogView
        lines={shown}
        empty={lines.length > 0 && shown.length === 0 ? s.noTrafficMatch : s.noTraffic}
      />
    </div>
  );
}
