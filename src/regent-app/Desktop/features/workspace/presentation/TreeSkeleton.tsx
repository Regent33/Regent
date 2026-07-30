// Placeholder rows shown while the root listing is in flight. A skeleton, not a
// spinner: it occupies the same shape the tree is about to take, so the real
// entries replace it in place instead of shoving a spinner out of the way.
//
// ponytail: Tailwind's own `animate-pulse` and a fixed set of widths — no
// animation library, no measuring, no shimmer gradient. Varying width and
// indent is what stops it reading as a progress bar.
import { Loader } from '@/shared/ui/Loader';

const ROWS = [
  { width: 'w-24', indent: 'ml-0' },
  { width: 'w-32', indent: 'ml-0' },
  { width: 'w-20', indent: 'ml-3' },
  { width: 'w-28', indent: 'ml-3' },
  { width: 'w-36', indent: 'ml-0' },
  { width: 'w-16', indent: 'ml-0' },
  { width: 'w-28', indent: 'ml-3' },
  { width: 'w-24', indent: 'ml-0' },
] as const;

export function TreeSkeleton({ label }: { label: string }) {
  return (
    // `role="status"`: a screen reader hears that the folder is loading, which
    // the decorative pulse below never announces on its own.
    <div role="status" aria-live="polite" className="relative">
      <div aria-hidden className="animate-pulse space-y-2 px-2 py-1.5">
        {ROWS.map((row, index) => (
          <div key={index} className={`flex items-center gap-1.5 ${row.indent}`}>
            <span className="size-3 shrink-0 rounded-xs bg-hover" />
            <span className={`h-2.5 rounded-xs bg-hover ${row.width}`} />
          </div>
        ))}
      </div>
      {/* Moving indicator over the skeleton: the pulse alone did not read as
          "working" on a slow folder. Not centred on the whole pane here — the
          tree is a narrow column, so vertically centring inside its own rows is
          where the eye already is. */}
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 flex items-center justify-center"
      >
        <span className="flex items-center gap-2 rounded-lg bg-surface/90 px-2.5 py-1.5 text-[11px] text-text-tertiary shadow-sm">
          <Loader />
          <span>{label}…</span>
        </span>
      </div>
    </div>
  );
}
