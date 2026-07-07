'use client';
// Frameless-window titlebar. The drag surface is the bar itself
// (data-tauri-drag-region — Tauri also gives it double-click-to-maximize);
// buttons sit on top and receive clicks normally. Window controls render only
// once we know we're in the shell (set post-mount to avoid a hydration
// mismatch between the static prerender and the Tauri runtime).
import { useEffect, useState } from 'react';
import { t } from '@/shared/i18n/t';
import { open as openOverlay } from '@/shared/state/overlays';
import { windowControls } from '@/shared/infrastructure/window/controls';
import { Button } from '@/shared/ui/Button';
import {
  ButlerIcon,
  ChevronDownIcon,
  CloseIcon,
  GearIcon,
  MinusIcon,
  PanelRightIcon,
  SquareIcon,
  UserIcon,
} from '@/shared/ui/icons';

export function Titlebar({ onAudio }: { onAudio?: () => void }) {
  const s = t().shell.titlebar;
  const [native, setNative] = useState(false);
  useEffect(() => setNative(windowControls.available()), []);

  return (
    <header
      data-tauri-drag-region
      className="flex h-9 shrink-0 items-stretch border-b border-stroke-tertiary"
    >
      <div className="flex items-center pl-2">
        <Button variant="text" size="sm" aria-label={s.sessionTitle}>
          <span className="text-sm">{s.sessionTitle}</span>
          <ChevronDownIcon className="size-3.5" />
        </Button>
      </div>
      <div className="ml-auto flex items-stretch">
        <Button variant="ghost" size="iconTitlebar" aria-label={s.butler} title={s.butler} onClick={onAudio}>
          <ButlerIcon />
        </Button>
        <Button variant="ghost" size="iconTitlebar" aria-label={s.account} title={s.account}>
          <UserIcon />
        </Button>
        <Button
          variant="ghost"
          size="iconTitlebar"
          aria-label={s.settings}
          title={s.settings}
          onClick={() => openOverlay('settings')}
        >
          <GearIcon />
        </Button>
        <Button variant="ghost" size="iconTitlebar" aria-label={s.rightPanel} title={s.rightPanel}>
          <PanelRightIcon />
        </Button>
        {native && (
          <>
            <Button
              variant="ghost"
              size="iconTitlebar"
              aria-label={s.minimize}
              title={s.minimize}
              onClick={windowControls.minimize}
            >
              <MinusIcon />
            </Button>
            <Button
              variant="ghost"
              size="iconTitlebar"
              aria-label={s.maximize}
              title={s.maximize}
              onClick={windowControls.toggleMaximize}
            >
              <SquareIcon />
            </Button>
            <Button
              variant="ghost"
              size="iconTitlebar"
              aria-label={s.close}
              title={s.close}
              className="hover:bg-danger hover:text-on-accent"
              onClick={windowControls.close}
            >
              <CloseIcon />
            </Button>
          </>
        )}
      </div>
    </header>
  );
}
