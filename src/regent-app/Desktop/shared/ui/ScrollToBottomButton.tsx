'use client';
// Floating "jump to latest" affordance — shown when the transcript is
// scrolled up past the near-bottom threshold (see ChatView's scroll tracking).
import { t } from '@/shared/i18n/t';
import { ChevronDownIcon } from '@/shared/ui/icons';

export function ScrollToBottomButton({ onClick }: { onClick: () => void }) {
  const label = t().chat.composer.scrollToBottom;
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className="absolute bottom-3 left-1/2 z-10 flex size-8 -translate-x-1/2 items-center justify-center rounded-full border border-stroke-secondary bg-surface text-text-secondary transition-colors motion-safe:animate-[fadeIn_120ms_ease-out] hover:bg-hover hover:text-text-primary"
      style={{ boxShadow: 'var(--shadow-elev)' }}
    >
      <ChevronDownIcon className="size-4" />
    </button>
  );
}
