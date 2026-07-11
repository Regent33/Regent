'use client';
// Composition root: features never import each other — the app layer joins
// the shell, Butler Mode, and the boot splash here. Butler mounts only while
// open, so the mic and audio graph exist exactly as long as the view does.
import { lazy, Suspense, type ReactNode, useEffect, useRef, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';
import { closeButler, useButlerOpen } from '@/shared/state/butler';
import { initTheme } from '@/shared/state/theme';
import { BootSplash } from '@/app/presentation/BootSplash';
import { Shell } from '@/features/shell/presentation/Shell';
import { refreshSessions } from '@/features/shell/viewmodels/useSessions';

// Lazy so Butler's heavier deps (globe.gl/three.js, mermaid) stay out of the
// boot chunk — the fetch starts on first toggle, exactly when the view mounts.
const ButlerView = lazy(() =>
  import('@/features/butler/presentation/ButlerView').then((m) => ({ default: m.ButlerView })),
);

const BOOT_POLL_MS = 400;
const BOOT_DEADLINE_MS = 15_000;

export function AppShell({ children }: { children: ReactNode }) {
  const butlerOpen = useButlerOpen();
  const [booted, setBooted] = useState(false);
  const butlerWasOpen = useRef(false);

  // Butler's voice calls land sessions in the shared store on disk through
  // the voice server — no notification reaches this webview. Refetch on EVERY
  // close path (X, Escape, composer toggle), and once more shortly after:
  // the voice server may still be flushing the session when the view closes.
  useEffect(() => {
    const wasOpen = butlerWasOpen.current;
    butlerWasOpen.current = butlerOpen;
    if (!wasOpen || butlerOpen) return;
    refreshSessions();
    const timer = setTimeout(refreshSessions, 2500);
    return () => clearTimeout(timer);
  }, [butlerOpen]);

  // Align the theme store with the choice the inline head script already
  // stamped onto <html>, so the Appearance selector reflects the live mode.
  useEffect(() => {
    initTheme();
  }, []);

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
          splash (mount churn invisible), then eases in as the splash leaves —
          no mixed half-rendered layers. Kept tight (300ms/no delay): the fade
          is pure added startup latency once the deacon has answered. */}
      <div
        className={`h-screen transition-opacity duration-300 ease-out ${
          booted ? 'opacity-100' : 'opacity-0'
        }`}
      >
        <Shell>{children}</Shell>
      </div>
      {butlerOpen && (
        <Suspense fallback={null}>
          <ButlerView onClose={closeButler} />
        </Suspense>
      )}
      <BootSplash done={booted} />
    </>
  );
}
