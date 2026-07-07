'use client';
// Full-screen, non-dismissible panel for a dead deacon backend — no fake
// spinner, no silent "gateway offline" footnote. Shows the verbatim error
// (never truncated/masked) and a Retry that reloads the webview, the
// simplest way to re-attempt a fresh spawn.
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { ErrorIcon } from '@/shared/ui/icons';

export function BootFailureOverlay({ message }: { message?: string }) {
  const s = t().shell.boot;

  return (
    <div
      role="alertdialog"
      aria-modal="true"
      aria-label={s.title}
      className="fixed inset-0 z-[80] flex items-center justify-center bg-scrim p-6 backdrop-blur-[2px]"
    >
      <div
        className="flex max-w-md flex-col items-center gap-3 rounded-xl border border-stroke-secondary bg-surface p-8 text-center motion-safe:animate-[fadeIn_150ms_ease-out]"
        style={{ boxShadow: 'var(--shadow-elev)' }}
      >
        <ErrorIcon className="size-6 text-danger" />
        <p className="text-sm font-semibold text-text-primary">{s.title}</p>
        <p className="break-words text-sm text-text-secondary">{message ?? s.message}</p>
        <Button variant="default" size="sm" className="mt-2" onClick={() => window.location.reload()}>
          {s.retry}
        </Button>
      </div>
    </div>
  );
}
