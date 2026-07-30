// Placeholder shown while a file's contents are in flight. The editor pane used
// to keep saying "Select a file to edit." for the whole read, so clicking a file
// in a big repo — or over a slow deacon — read as a click that had missed.
//
// Same reasoning as TreeSkeleton next door: a skeleton in the shape of the thing
// arriving, not a spinner, so the real editor replaces it in place. Line-shaped
// rows with a gutter, because that is what a code buffer looks like.
//
// ponytail: Tailwind's own `animate-pulse` and a fixed width table — no
// animation library, no measuring. Varying widths are what stop it reading as a
// progress bar.
import { Loader } from '@/shared/ui/Loader';

const LINES = [
  'w-2/5',
  'w-3/5',
  'w-1/4',
  'w-4/5',
  'w-1/2',
  'w-2/3',
  'w-1/3',
  'w-3/4',
  'w-2/5',
  'w-1/2',
  'w-3/5',
  'w-1/4',
] as const;

export function EditorSkeleton({ path, label }: { path: string; label: string }) {
  const name = path.replace(/\\/g, '/').split('/').pop() ?? path;
  return (
    // `role="status"` + the visible label: a screen reader is told the file is
    // loading, which a purely decorative pulse would never announce.
    <div role="status" aria-live="polite" className="flex min-w-0 flex-1 flex-col">
      <div className="flex items-center gap-1.5 border-b border-stroke-tertiary px-2 py-1">
        <span className="truncate text-[12px] text-text-tertiary">
          {label} {name}…
        </span>
      </div>
      {/* The skeleton alone was too quiet to read as activity — asked for
          something visibly moving in the MIDDLE of the pane. So: skeleton
          underneath for shape, one centred indicator on top for motion. The
          wrapper is `relative` and the overlay `pointer-events-none`, so it
          cannot swallow a click meant for the editor arriving behind it. */}
      <div className="relative min-h-0 flex-1">
        <div aria-hidden className="animate-pulse space-y-2.5 p-3">
          {LINES.map((width, index) => (
            <div key={index} className="flex items-center gap-3">
              <span className="h-2.5 w-4 shrink-0 rounded-xs bg-hover opacity-60" />
              <span className={`h-2.5 rounded-xs bg-hover ${width}`} />
            </div>
          ))}
        </div>
        <div
          aria-hidden
          className="pointer-events-none absolute inset-0 flex items-center justify-center"
        >
          {/* `bg-surface` behind it, or the pulsing rows read straight through
              the dots and both look like noise. */}
          <span className="flex items-center gap-2 rounded-lg bg-surface/90 px-3 py-2 text-[11px] text-text-tertiary shadow-sm">
            <Loader />
            <span className="truncate">{label}…</span>
          </span>
        </div>
      </div>
    </div>
  );
}
