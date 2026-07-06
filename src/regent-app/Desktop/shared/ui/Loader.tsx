// The loading indicator — three staggered swelling dots (loader-dot
// keyframes in globals.css: strong opacity swing + a 2px rise, so it reads
// at glance). Never the literal text "Loading…"; reduced-motion users get
// three static visible dots (the global kill leaves the 0.7 base opacity).
import { t } from '@/shared/i18n/t';

export function Loader({ className = '' }: { className?: string }) {
  return (
    <span role="status" aria-label={t().ui.loading} className={`inline-flex items-center gap-1 ${className}`}>
      {[0, 1, 2].map((i) => (
        <span
          key={i}
          className="loader-dot size-1.5 rounded-full bg-text-secondary"
          style={{ animationDelay: `${i * 160}ms` }}
        />
      ))}
    </span>
  );
}
