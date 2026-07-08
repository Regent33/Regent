'use client';
// The app frame: titlebar / rail + main (watermark behind) / status bar.
// This is the client boundary — the root layout stays a thin server shell.
// The main pane re-animates per route (fade + rise), keyed on the pathname.
import { useEffect, useState, type ReactNode } from 'react';
import { usePathname } from 'next/navigation';
import { Titlebar } from '@/features/shell/presentation/Titlebar';
import { LeftRail } from '@/features/shell/presentation/LeftRail';
import { StatusBar } from '@/features/shell/presentation/StatusBar';
import { CommandPalette } from '@/features/shell/presentation/CommandPalette';
import { OverlayHost } from '@/features/shell/presentation/OverlayHost';
import { BootFailureOverlay } from '@/features/shell/presentation/BootFailureOverlay';
import { KeybindPanel } from '@/features/shell/presentation/KeybindPanel';
import { usePalette } from '@/features/shell/viewmodels/usePalette';
import { useBootHealth } from '@/features/shell/viewmodels/useBootHealth';
import { useOverlayEsc } from '@/shared/state/overlays';
import { useTurnCompletionNotify } from '@/shared/infrastructure/notify';

/** True while the event's target is a place the user is typing — the "?" key
 * (Shift+/) must not hijack a literal question mark mid-sentence. */
function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable;
}

export function Shell({
  children,
  onButlerToggle,
}: {
  children: ReactNode;
  onButlerToggle?: () => void;
}) {
  const palette = usePalette();
  const pathname = usePathname();
  const boot = useBootHealth();
  const [keybindsOpen, setKeybindsOpen] = useState(false);
  useOverlayEsc();
  useTurnCompletionNotify();

  // "?" opens the keybinds panel from anywhere (not while typing); Esc closes
  // it. This lives here rather than in the overlays store — see
  // shared/state/keybinds.ts for why the map stays descriptive-only.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === '?' && !isTypingTarget(e.target) && !e.ctrlKey && !e.metaKey && !e.altKey) {
        e.preventDefault();
        setKeybindsOpen((open) => !open);
      } else if (e.key === 'Escape') {
        setKeybindsOpen((open) => (open ? false : open));
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  return (
    <div className="flex h-screen flex-col overflow-hidden bg-bg text-text-primary">
      <Titlebar onAudio={onButlerToggle} />
      <div className="flex min-h-0 flex-1">
        <LeftRail />
        <main className="relative min-w-0 flex-1 overflow-y-auto overflow-x-hidden bg-surface">
          <div key={pathname} className="relative h-full motion-safe:animate-[fadeIn_180ms_ease-out]">
            {children}
          </div>
        </main>
      </div>
      <StatusBar />
      <CommandPalette palette={palette} />
      <OverlayHost />
      {keybindsOpen && <KeybindPanel onClose={() => setKeybindsOpen(false)} />}
      {boot.dead && <BootFailureOverlay message={boot.message} />}
    </div>
  );
}
