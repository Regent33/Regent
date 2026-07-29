// Placeholder rows shown while the root listing is in flight. A skeleton, not a
// spinner: it occupies the same shape the tree is about to take, so the real
// entries replace it in place instead of shoving a spinner out of the way.
//
// ponytail: Tailwind's own `animate-pulse` and a fixed set of widths — no
// animation library, no measuring, no shimmer gradient. Varying width and
// indent is what stops it reading as a progress bar.
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

export function TreeSkeleton() {
  return (
    <div aria-hidden className="animate-pulse space-y-2 px-2 py-1.5">
      {ROWS.map((row, index) => (
        <div key={index} className={`flex items-center gap-1.5 ${row.indent}`}>
          <span className="size-3 shrink-0 rounded-xs bg-hover" />
          <span className={`h-2.5 rounded-xs bg-hover ${row.width}`} />
        </div>
      ))}
    </div>
  );
}
