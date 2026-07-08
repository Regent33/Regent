'use client';
// Grouped section list shared by the Skills and Toolsets tabs: one uppercase
// heading per category, each followed by its rows (name + description left,
// toggle right). Categories are pre-computed by the caller (see SkillsView) —
// this component only buckets and renders.
import type { ReactNode } from 'react';

export interface GroupedItem {
  readonly key: string;
  readonly name: string;
  readonly description?: string;
  readonly category: string;
  readonly dimmed?: boolean;
  readonly toggle: ReactNode;
}

function Row({ name, description, dimmed, toggle }: Omit<GroupedItem, 'category' | 'key'>) {
  return (
    <div
      className={`flex items-center gap-3 rounded-[6px] px-2.5 py-2 transition-colors hover:bg-hover ${
        dimmed === true ? 'opacity-50' : ''
      }`}
    >
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm text-text-primary">{name}</p>
        {description !== undefined && <p className="truncate text-xs text-text-tertiary">{description}</p>}
      </div>
      {toggle}
    </div>
  );
}

export function GroupedRows({
  items,
  emptyLabel,
  otherLast,
}: {
  items: readonly GroupedItem[];
  emptyLabel: string;
  otherLast?: string;
}) {
  if (items.length === 0) {
    return <p className="px-3 py-4 text-xs text-text-tertiary">{emptyLabel}</p>;
  }

  const groups = new Map<string, GroupedItem[]>();
  for (const item of items) {
    const list = groups.get(item.category) ?? [];
    list.push(item);
    groups.set(item.category, list);
  }

  const ordered = [...groups.entries()].sort(([a], [b]) => {
    if (a === otherLast) return 1;
    if (b === otherLast) return -1;
    return a.localeCompare(b);
  });

  return (
    <div className="flex flex-col gap-5 p-3">
      {ordered.map(([category, rows]) => (
        <section key={category}>
          <h3 className="px-2.5 pb-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-text-tertiary">
            {category}
          </h3>
          <div className="flex flex-col">
            {rows.map(({ key, ...item }) => (
              <Row key={key} {...item} />
            ))}
          </div>
        </section>
      ))}
    </div>
  );
}
