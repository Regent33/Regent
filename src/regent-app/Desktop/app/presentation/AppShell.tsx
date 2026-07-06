'use client';
// Composition root: features never import each other — the app layer joins
// the shell, Butler Mode, and the boot splash here. Butler mounts only while
// open, so the mic and audio graph exist exactly as long as the view does.
import { type ReactNode, useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';
import { BootSplash } from '@/app/presentation/BootSplash';
import { Shell } from '@/features/shell/presentation/Shell';
import { ButlerView } from '@/features/butler/presentation/ButlerView';

const BOOT_POLL_MS = 400;
const BOOT_DEADLINE_MS = 15_000;

export function AppShell({ children }: { children: ReactNode }) {
  const [butlerOpen, setButlerOpen] = useState(false);
  const [booted, setBooted] = useState(false);

  // Splash until the deacon answers (or the deadline passes — never trap the
  // user on the splash; failures surface in the shell's status bar instead).
  useEffect(() => {
    let alive = true;
    if (!isTauri()) {
      const timer = setTimeout(() => setBooted(true), 400);
      return () => clearTimeout(timer);
    }
    const deadline = Date.now() + BOOT_DEADLINE_MS;
    void (async () => {
      while (alive && Date.now() < deadline) {
        const status = await deaconRequest('status.get', {});
        if (!alive) return;
        if (status.ok) break;
        await new Promise((r) => setTimeout(r, BOOT_POLL_MS));
      }
      if (alive) setBooted(true);
    })();
    return () => {
      alive = false;
    };
  }, []);

  return (
    <>
      {/* Coordinated crossfade: the shell holds at opacity 0 under the opaque
          splash (mount churn invisible), then eases in just after the splash
          starts leaving — no mixed half-rendered layers. */}
      <div
        className={`h-screen transition-opacity duration-700 ease-out ${
          booted ? 'opacity-100 delay-150' : 'opacity-0'
        }`}
      >
        <Shell onButlerToggle={() => setButlerOpen((open) => !open)}>{children}</Shell>
      </div>
      {butlerOpen && <ButlerView onClose={() => setButlerOpen(false)} />}
      <BootSplash done={booted} />
    </>
  );
}
