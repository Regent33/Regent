'use client';
// Composition root: features never import each other — the app layer joins
// the shell and Butler Mode here. Butler mounts only while open, so the mic
// and audio graph exist exactly as long as the view does.
import { type ReactNode, useState } from 'react';
import { Shell } from '@/features/shell/presentation/Shell';
import { ButlerView } from '@/features/butler/presentation/ButlerView';

export function AppShell({ children }: { children: ReactNode }) {
  const [butlerOpen, setButlerOpen] = useState(false);

  return (
    <>
      <Shell onButlerToggle={() => setButlerOpen((open) => !open)}>{children}</Shell>
      {butlerOpen && <ButlerView onClose={() => setButlerOpen(false)} />}
    </>
  );
}
