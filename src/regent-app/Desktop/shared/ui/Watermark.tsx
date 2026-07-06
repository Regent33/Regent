// Faint wordmark texture behind content panes — never behind the hero (two
// stacked wordmarks read as broken art, not branding).
import { t } from '@/shared/i18n/t';

export function Watermark() {
  return (
    <div
      aria-hidden
      className="pointer-events-none absolute inset-0 flex select-none items-center justify-center overflow-hidden"
    >
      <span
        className="whitespace-nowrap text-[15vw] font-bold leading-none text-text-primary opacity-[0.025]"
        style={{ fontFamily: 'var(--font-display)' }}
      >
        {t().home.wordmark}
      </span>
    </div>
  );
}
