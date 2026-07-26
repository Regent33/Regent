'use client';
// Recursive workspace file tree. Unlike the Artifacts list (a flat slug→files
// map), this is arbitrary depth and lazy: a directory's children are fetched
// the first time it expands, then cached by the viewmodel.
import { ChevronDownIcon } from '@/shared/ui/icons';
import type { TreeEntry } from '@/features/workspace/domain/workspaceModel';

interface FileTreeProps {
  readonly levels: Record<string, readonly TreeEntry[]>;
  readonly expanded: ReadonlySet<string>;
  readonly openPath?: string;
  /** Highlighted node — a folder counts, which is what makes "new file HERE"
   * work. Distinct from `openPath` (the file loaded in the editor). */
  readonly selectedPath?: string;
  readonly onToggle: (path: string) => void;
  readonly onOpen: (path: string) => void;
  readonly onSelect: (path: string, isDir: boolean) => void;
  /** Directory whose children to render; '' is the workspace root. */
  readonly dir?: string;
  readonly depth?: number;
}

export function FileTree({
  levels,
  expanded,
  openPath,
  selectedPath,
  onToggle,
  onOpen,
  onSelect,
  dir = '',
  depth = 0,
}: FileTreeProps) {
  const entries = levels[dir];
  if (entries === undefined) return null;

  return (
    <ul>
      {entries.map((entry) => {
        const isOpenDir = entry.isDir && expanded.has(entry.path);
        // A folder can be selected too — that's what makes "new file here"
        // land where the user is pointing.
        const selected = entry.path === selectedPath || (!entry.isDir && entry.path === openPath);
        return (
          <li key={entry.path}>
            <button
              type="button"
              title={entry.path}
              aria-current={selected ? 'true' : undefined}
              // Indent by depth rather than nesting padding, so deep trees
              // don't lose horizontal room to stacked containers.
              style={{ paddingLeft: `${depth * 12 + 8}px` }}
              className={`flex w-full cursor-pointer items-center gap-1 rounded-[4px] py-0.5 pr-2 text-left text-[12px] hover:bg-hover ${
                selected ? 'bg-hover text-text-primary' : 'text-text-secondary'
              }`}
              onClick={() => {
                onSelect(entry.path, entry.isDir);
                if (entry.isDir) onToggle(entry.path);
                else onOpen(entry.path);
              }}
            >
              {entry.isDir ? (
                <ChevronDownIcon
                  className={`size-3 shrink-0 transition-transform ${isOpenDir ? '' : '-rotate-90'}`}
                />
              ) : (
                // Keeps file names aligned with directory labels.
                <span className="size-3 shrink-0" />
              )}
              <span className="truncate">{entry.name}</span>
            </button>
            {isOpenDir && (
              <FileTree
                levels={levels}
                expanded={expanded}
                openPath={openPath}
                selectedPath={selectedPath}
                onToggle={onToggle}
                onOpen={onOpen}
                onSelect={onSelect}
                dir={entry.path}
                depth={depth + 1}
              />
            )}
          </li>
        );
      })}
    </ul>
  );
}
