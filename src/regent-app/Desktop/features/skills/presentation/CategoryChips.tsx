'use client';
// Category-chip filter row shared by the Skills and Toolsets tabs: "All"
// (with the tab's total) first, then one chip per category with its count.
// Clicking a chip filters both the chip row's own selection and the grouped
// sections below it.
export function CategoryChips({
  counts,
  chip,
  onChip,
  allLabel,
  total,
}: {
  counts: ReadonlyMap<string, number>;
  chip?: string;
  onChip: (chip?: string) => void;
  allLabel: string;
  total: number;
}) {
  const chipClass = (own?: string) =>
    `rounded-full px-2.5 py-1 text-xs transition-colors ${
      chip === own ? 'bg-accent text-on-accent' : 'bg-hover text-text-secondary hover:text-text-primary'
    }`;
  return (
    <div className="flex flex-wrap gap-1.5 border-b border-stroke-tertiary px-3 py-2">
      <button type="button" className={chipClass(undefined)} onClick={() => onChip(undefined)}>
        {allLabel} {total}
      </button>
      {[...counts.entries()].sort(([a], [b]) => a.localeCompare(b)).map(([name, count]) => (
        <button key={name} type="button" className={chipClass(name)} onClick={() => onChip(name)}>
          {name} {count}
        </button>
      ))}
    </div>
  );
}
