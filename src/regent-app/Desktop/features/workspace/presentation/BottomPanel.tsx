'use client';
// The workspace's bottom panel — VS Code's Terminal / Output / Debug Console,
// under the editor. Phase 1 is the shell: tab bar, drag-to-resize, close. The
// tabs' contents arrive in later phases; each renders its own placeholder until
// then rather than the panel pretending to be finished.
//
// ponytail: no tab library. Three buttons and a conditional are the whole
// mechanism, and `role="tablist"` is what makes it a real tab set to a screen
// reader — a dependency would add nothing except a dependency.
import { useEffect, useRef, useState } from 'react';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { CloseIcon } from '@/shared/ui/icons';
import {
  PANEL_MIN_HEIGHT,
  PANEL_TABS,
  type PanelTab,
  clampPanelHeight,
} from '@/features/workspace/domain/panelModel';
import { useDragSize } from '@/features/workspace/viewmodels/useDragSize';

export interface BottomPanelProps {
  height: number;
  /** Available vertical space, so the drag can leave the editor some room. */
  available: number;
  onHeightChange: (height: number) => void;
  onClose: () => void;
  /** Rendered once per tab, with whether that tab is the visible one.
   *
   * Every tab stays MOUNTED — the panel hides the inactive ones with CSS. It
   * used to render the active tab alone, so clicking Output closed every
   * terminal and killed whatever was running in them. Returning `undefined`
   * for a tab falls back to its placeholder. */
  children?: (tab: PanelTab, visible: boolean) => React.ReactNode;
}

export function BottomPanel({
  height,
  available,
  onHeightChange,
  onClose,
  children,
}: BottomPanelProps) {
  const s = t().workspace.panel;
  const [tab, setTab] = useState<PanelTab>('terminal');
  // `direction: -1`, `axis: 'y'` — the handle is the panel's TOP edge, so
  // dragging UP has to make it taller.
  const drag = useDragSize(height, PANEL_MIN_HEIGHT, Math.max(PANEL_MIN_HEIGHT, available), -1, 'y');

  // Publish the dragged height upward without fighting the drag: the hook owns
  // the live value, the parent only needs the settled one.
  const notify = useRef(onHeightChange);
  notify.current = onHeightChange;
  useEffect(() => {
    notify.current(clampPanelHeight(drag.size, available));
  }, [drag.size, available]);

  const label: Record<PanelTab, string> = {
    terminal: s.terminal,
    output: s.output,
    debug: s.debugConsole,
  };

  return (
    <section
      aria-label={s.title}
      className="relative flex shrink-0 flex-col border-t border-stroke-tertiary bg-bg"
      style={{ height: `${clampPanelHeight(drag.size, available)}px` }}
    >
      {/* Top-edge drag handle. Its own row so it never overlaps the tab
          buttons — a handle that steals the first click on a tab is worse than
          no handle. */}
      <div
        role="separator"
        aria-orientation="horizontal"
        className="absolute inset-x-0 -top-0.5 z-10 h-1 cursor-row-resize hover:bg-accent/40"
        {...drag.handleProps}
      />

      <div
        role="tablist"
        aria-label={s.title}
        className="flex items-center gap-0.5 border-b border-stroke-tertiary px-1.5 py-1"
      >
        {PANEL_TABS.map((name) => (
          <button
            key={name}
            type="button"
            role="tab"
            aria-selected={tab === name}
            className={`rounded-sm px-2 py-0.5 text-[11px] uppercase tracking-wide ${
              tab === name
                ? 'bg-hover text-text-primary'
                : 'text-text-tertiary hover:text-text-secondary'
            }`}
            onClick={() => setTab(name)}
          >
            {label[name]}
          </button>
        ))}
        <span className="flex-1" />
        <Button size="iconSm" variant="ghost" aria-label={s.close} title={s.close} onClick={onClose}>
          <CloseIcon />
        </Button>
      </div>

      {/* `min-h-0` so a tall child scrolls inside the panel instead of pushing
          the panel past the height the drag just set. */}
      {PANEL_TABS.map((name) => (
        <div
          key={name}
          role="tabpanel"
          aria-label={label[name]}
          hidden={name !== tab}
          className={`${name === tab ? 'flex' : 'hidden'} min-h-0 flex-1 flex-col overflow-auto`}
        >
          {children?.(name, name === tab) ?? (
            <p className="p-3 text-[12px] text-text-tertiary">{s.comingSoon[name]}</p>
          )}
        </div>
      ))}
    </section>
  );
}
