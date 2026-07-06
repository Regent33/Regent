// The loading indicator — three fading dots, CSS-only. Never the literal
// text "Loading…"; reduced-motion users get a static glyph (globals.css
// kills the animation, leaving three visible dots).
import { t } from '@/shared/i18n/t';

export function Loader({ className = '' }: { className?: string }) {
  return (
    <span role="status" aria-label={t().ui.loading} className={`inline-flex items-center gap-1 ${className}`}>
      {[0, 1, 2].map((i) => (
        <span
          key={i}
          className="size-1.5 rounded-full bg-text-tertiary motion-safe:animate-pulse"
          style={{ animationDelay: `${i * 150}ms` }}
        />
      ))}
    </span>
  );
}
