'use client';
// The app frame: titlebar / rail + main (watermark behind) / status bar.
// This is the client boundary — the root layout stays a thin server shell.
// The main pane re-animates per route (fade + rise), keyed on the pathname.
import type { ReactNode } from 'react';
import { usePathname } from 'next/navigation';
import { Titlebar } from '@/features/shell/presentation/Titlebar';
import { LeftRail } from '@/features/shell/presentation/LeftRail';
import { StatusBar } from '@/features/shell/presentation/StatusBar';
import { CommandPalette } from '@/features/shell/presentation/CommandPalette';
import { usePalette } from '@/features/shell/viewmodels/usePalette';

export function Shell({
  children,
  onButlerToggle,
}: {
  children: ReactNode;
  onButlerToggle?: () => void;
}) {
  const palette = usePalette();
  const pathname = usePathname();

  return (
    <div className="flex h-screen flex-col overflow-hidden bg-bg text-text-primary">
      <Titlebar onAudio={onButlerToggle} />
      <div className="flex min-h-0 flex-1">
        <LeftRail />
        <main className="relative min-w-0 flex-1 overflow-y-auto bg-surface">
          <div key={pathname} className="relative h-full motion-safe:animate-[fadeIn_180ms_ease-out]">
            {children}
          </div>
        </main>
      </div>
      <StatusBar />
      <CommandPalette palette={palette} />
    </div>
  );
}
