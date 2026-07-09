'use client';
// Left pane — artifacts grouped by slug (one run), newest slug first
// (already sorted by the viewmodel), filtered by file name across all
// groups. A group that has no matching files after filtering is hidden.
// Each group is COLLAPSED by default behind its heading (SESSIONS-style
// chevron); a live search forces groups open so matches stay visible.
import { useState } from 'react';
import { t } from '@/shared/i18n/t';
import { ListRow } from '@/shared/ui/ListRow';
import { EmptyState } from '@/shared/ui/EmptyState';
import { ChevronDownIcon, FileIcon } from '@/shared/ui/icons';
import type { ArtifactGroup } from '@/features/artifacts/viewmodels/useArtifactsList';

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function ArtifactList({
  groups,
  query,
  selected,
  onSelect,
}: {
  groups: readonly ArtifactGroup[];
  query: string;
  selected?: string;
  onSelect: (rel: string) => void;
}) {
  const s = t().artifacts;
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());
  const q = query.trim().toLowerCase();
  const toggle = (slug: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(slug)) next.delete(slug);
      else next.add(slug);
      return next;
    });
  const filtered = groups
    .map((group) => ({
      ...group,
      files: q === '' ? group.files : group.files.filter((f) => f.name.toLowerCase().includes(q)),
    }))
    .filter((group) => group.files.length > 0);

  if (filtered.length === 0) {
    return (
      <div className="mt-4">
        <EmptyState title={q === '' ? s.empty : s.noMatches} hint={q === '' ? s.emptyHint : undefined} />
      </div>
    );
  }

  return (
    <>
      {filtered.map((group) => {
        const open = q !== '' || expanded.has(group.slug);
        return (
          <section key={group.slug} className="mb-3">
            <button
              type="button"
              aria-expanded={open}
              className="flex w-full cursor-pointer items-center gap-1 px-2.5 pb-1 pt-2 text-left text-[10px] font-semibold uppercase tracking-[0.08em] text-text-tertiary hover:text-text-secondary"
              onClick={() => toggle(group.slug)}
            >
              <ChevronDownIcon className={`size-3 shrink-0 transition-transform ${open ? '' : '-rotate-90'}`} />
              <span className="truncate">
                {group.slug} · {group.files.length} {s.filesCount}
              </span>
            </button>
            {open &&
              group.files.map((file) => (
                <ListRow
                  key={file.rel}
                  icon={<FileIcon className="size-4" />}
                  label={file.name}
                  description={formatBytes(file.bytes)}
                  active={selected === file.rel}
                  dense
                  onClick={() => onSelect(file.rel)}
                />
              ))}
          </section>
        );
      })}
    </>
  );
}
