// Faint full-bleed wordmark behind the main pane — pure decoration.
import { t } from '@/shared/i18n/t';

export function Watermark() {
  return (
    <div
      aria-hidden
      className="pointer-events-none absolute inset-0 flex select-none items-center justify-center overflow-hidden"
    >
      <span
        className="whitespace-nowrap text-[18vw] font-bold leading-none text-accent opacity-[0.045]"
        style={{ fontFamily: 'var(--font-display)' }}
      >
        {t().home.wordmark}
      </span>
    </div>
  );
}
