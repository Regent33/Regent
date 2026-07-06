'use client';
// The app frame: titlebar / rail + main (watermark behind) / status bar.
// This is the client boundary — the root layout stays a thin server shell.
import type { ReactNode } from 'react';
import { Titlebar } from '@/features/shell/presentation/Titlebar';
import { LeftRail } from '@/features/shell/presentation/LeftRail';
import { StatusBar } from '@/features/shell/presentation/StatusBar';
import { CommandPalette } from '@/features/shell/presentation/CommandPalette';
import { Watermark } from '@/features/shell/presentation/Watermark';
import { usePalette } from '@/features/shell/viewmodels/usePalette';

export function Shell({
  children,
  onButlerToggle,
}: {
  children: ReactNode;
  onButlerToggle?: () => void;
}) {
  const palette = usePalette();

  return (
    <div className="flex h-screen flex-col overflow-hidden bg-bg text-text-primary">
      <Titlebar onAudio={onButlerToggle} />
      <div className="flex min-h-0 flex-1">
        <LeftRail />
        <main className="relative min-w-0 flex-1 overflow-y-auto">
          <Watermark />
          <div className="relative h-full">{children}</div>
        </main>
      </div>
      <StatusBar />
      <CommandPalette palette={palette} />
    </div>
  );
}
