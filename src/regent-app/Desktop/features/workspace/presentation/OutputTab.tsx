'use client';
// The Output tab: two channels behind a dropdown, like VS Code's.
//
//   Agent tools — live, from the bus (tool.start / tool.complete)
//   Deacon log  — polled, from `logs.tail`
//
// Both stay mounted whichever is showing, so switching channels does not throw
// away tool activity that arrived while you were reading the log.
import { useCallback, useEffect, useState } from 'react';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { deaconRequest } from '@/shared/infrastructure/rpc/client';
import { subscribe } from '@/shared/state/deaconBus';
import {
  NO_LINES,
  type OutputLine,
  appendLines,
  logTone,
  toolLine,
} from '@/features/workspace/domain/outputLines';
import { LogView } from '@/features/workspace/presentation/LogView';

type Channel = 'tools' | 'log';

/** The log is a file, so it has to be asked. Only while the tab is visible —
 * a poll that runs behind a hidden panel is pure waste. */
const POLL_MS = 3_000;

export function OutputTab({ visible }: { visible: boolean }) {
  const s = t().workspace.panel;
  const [channel, setChannel] = useState<Channel>('tools');
  const [tools, setTools] = useState<readonly OutputLine[]>(NO_LINES);
  const [log, setLog] = useState<readonly OutputLine[]>(NO_LINES);
  const [logError, setLogError] = useState<string>();

  // Tool activity is collected whether or not this tab is open: it is not
  // replayable, so anything missed while looking elsewhere is gone for good.
  useEffect(
    () =>
      subscribe({}, (event) => {
        const line = toolLine(event.method, event.params);
        if (line === undefined) return;
        setTools((current) => appendLines(current, [line.text], line.tone));
      }),
    [],
  );

  const loadLog = useCallback(async () => {
    const result = await deaconRequest('logs.tail', { limit: 400 });
    if (!result.ok) {
      setLogError(result.error.message);
      return;
    }
    setLogError(undefined);
    const raw = (result.value as { lines?: unknown }).lines;
    const texts = Array.isArray(raw) ? raw.filter((l): l is string => typeof l === 'string') : [];
    // Replaced wholesale rather than appended: a tail overlaps what is already
    // shown, and appending would duplicate every line on every poll.
    setLog(texts.map((text, index) => ({ id: index + 1, text, tone: logTone(text) })));
  }, []);

  useEffect(() => {
    if (!visible || channel !== 'log') return;
    void loadLog();
    const timer = setInterval(() => void loadLog(), POLL_MS);
    return () => clearInterval(timer);
  }, [visible, channel, loadLog]);

  const onTools = channel === 'tools';
  const empty = logError ?? (onTools ? s.noToolActivity : s.noLogLines);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-2 border-b border-stroke-tertiary px-2 py-1">
        <label className="text-[10px] uppercase tracking-wide text-text-tertiary" htmlFor="output-channel">
          {s.channel}
        </label>
        <select
          id="output-channel"
          value={channel}
          className="rounded-sm bg-transparent px-1 py-0.5 text-[11px] text-text-primary outline-none"
          onChange={(e) => setChannel(e.target.value === 'log' ? 'log' : 'tools')}
        >
          <option value="tools">{s.channelTools}</option>
          <option value="log">{s.channelLog}</option>
        </select>
        <span className="flex-1" />
        {onTools ? (
          <Button size="sm" variant="ghost" onClick={() => setTools(NO_LINES)}>
            {s.clear}
          </Button>
        ) : (
          <Button size="sm" variant="ghost" onClick={() => void loadLog()}>
            {s.refresh}
          </Button>
        )}
      </div>
      <LogView lines={onTools ? tools : log} empty={empty} />
    </div>
  );
}
