'use client';
// The Terminal tab: a strip of terminals down the right, each one a live shell.
// Every instance stays mounted — see TerminalInstance for why unmounting would
// kill the running process.
import { useState } from 'react';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { CloseIcon, PlusIcon } from '@/shared/ui/icons';
import {
  NO_TERMINALS,
  activate,
  addTerminal,
  closeTerminal,
  ensureOne,
} from '@/features/workspace/domain/terminalTabs';
import { TerminalInstance } from '@/features/workspace/presentation/TerminalInstance';

export function TerminalTab({ sessionId }: { sessionId: string | undefined }) {
  const s = t().workspace.panel;
  // Opens with one terminal already running: making someone press "+" before
  // they can type is a step with no purpose.
  const [state, setState] = useState(() => ensureOne(NO_TERMINALS));

  const add = () => setState(addTerminal);
  const close = (id: number) => setState((current) => closeTerminal(current, id));

  return (
    <div className="flex h-full min-h-0">
      <div className="relative min-w-0 flex-1">
        {state.tabs.map((tab) => (
          <TerminalInstance
            key={tab.id}
            sessionId={sessionId}
            visible={tab.id === state.activeId}
            // The shell exited on its own (`exit`, or a crash). The tab stays so
            // the last output is still readable — closing it would delete the
            // evidence of why it died.
            onExit={() => undefined}
            onNewTerminal={add}
          />
        ))}
        {state.tabs.length === 0 && (
          <p className="p-3 text-[12px] text-text-tertiary">{s.noTerminals}</p>
        )}
      </div>

      {/* Down the right, like VS Code. Vertical so a long-running terminal's
          output keeps the full width. */}
      <div className="flex w-28 shrink-0 flex-col gap-0.5 border-l border-stroke-tertiary p-1">
        <div className="flex items-center justify-between px-1">
          <span className="text-[10px] uppercase tracking-wide text-text-tertiary">
            {s.terminal}
          </span>
          <Button size="iconSm" variant="ghost" aria-label={s.newTerminal} title={s.newTerminal} onClick={add}>
            <PlusIcon />
          </Button>
        </div>
        {state.tabs.map((tab) => (
          <div
            key={tab.id}
            className={`group flex items-center gap-1 rounded-sm px-1.5 py-0.5 text-[11px] ${
              tab.id === state.activeId
                ? 'bg-hover text-text-primary'
                : 'text-text-tertiary hover:text-text-secondary'
            }`}
          >
            <button
              type="button"
              className="min-w-0 flex-1 truncate text-left"
              aria-current={tab.id === state.activeId}
              onClick={() => setState((current) => activate(current, tab.id))}
            >
              {s.terminal} {tab.label}
            </button>
            <button
              type="button"
              aria-label={`${s.closeTerminal} ${tab.label}`}
              title={s.closeTerminal}
              // Always reachable, not hover-only: a hover-gated close is
              // unusable by keyboard and invisible on a touch screen.
              className="shrink-0 text-text-tertiary hover:text-text-primary"
              onClick={() => close(tab.id)}
            >
              <CloseIcon className="size-3" />
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
