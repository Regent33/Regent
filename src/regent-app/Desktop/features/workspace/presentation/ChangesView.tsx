'use client';
// Every uncommitted change in the workspace, file by file, rendered with the
// same red/teal treatment the chat uses for tool-call diffs — so "25 changes"
// is something you can actually read before committing it.
import { useEffect, useMemo, useState } from 'react';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { ErrorState } from '@/shared/ui/ErrorState';
import { Loader } from '@/shared/ui/Loader';
import { CloseIcon } from '@/shared/ui/icons';
import { deaconRequest } from '@/shared/infrastructure/rpc/client';
import { type DiffFile, parseUnifiedDiff } from '@/features/workspace/domain/parseDiff';

interface ChangesViewProps {
  readonly sessionId: string | undefined;
  readonly onClose: () => void;
  /** Jump to a file in the editor from its diff header. */
  readonly onOpenFile: (path: string) => void;
}

export function ChangesView({ sessionId, onClose, onOpenFile }: ChangesViewProps) {
  const s = t().workspace;
  const [diff, setDiff] = useState<string>();
  const [error, setError] = useState<string>();

  useEffect(() => {
    if (sessionId === undefined) return;
    let alive = true;
    void deaconRequest('git.diff', { session_id: sessionId }).then((result) => {
      if (!alive) return;
      if (!result.ok) {
        setError(result.error.message);
        return;
      }
      setDiff((result.value as { diff?: string })?.diff ?? '');
    });
    return () => {
      alive = false;
    };
  }, [sessionId]);

  const files: DiffFile[] = useMemo(
    () => (diff === undefined ? [] : parseUnifiedDiff(diff)),
    [diff],
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-1.5 border-b border-stroke-tertiary px-2 py-1">
        <span className="flex-1 truncate text-[11px] text-text-tertiary">
          {s.changesTitle}
          {files.length > 0 && ` · ${s.changes(files.length)}`}
        </span>
        <Button size="iconSm" variant="ghost" aria-label={s.close} title={s.close} onClick={onClose}>
          <CloseIcon />
        </Button>
      </div>

      <div className="min-h-0 flex-1 overflow-auto">
        {error !== undefined && (
          <div className="p-2">
            <ErrorState compact description={error} />
          </div>
        )}
        {error === undefined && diff === undefined && (
          <div className="flex justify-center py-6">
            <Loader />
          </div>
        )}
        {diff !== undefined && files.length === 0 && (
          <p className="p-3 text-[12px] text-text-tertiary">{s.noChanges}</p>
        )}

        {files.map((file) => (
          <section key={file.path} className="border-b border-stroke-tertiary last:border-b-0">
            <button
              type="button"
              className="flex w-full cursor-pointer items-center gap-2 px-2 py-1.5 text-left hover:bg-hover"
              onClick={() => onOpenFile(file.path)}
              title={s.openInEditor}
            >
              <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-text-secondary">
                {file.path}
              </span>
              {file.adds > 0 && <span className="font-mono text-[11px] text-accent">+{file.adds}</span>}
              {file.dels > 0 && <span className="font-mono text-[11px] text-danger">−{file.dels}</span>}
            </button>

            {/* Same w-max/min-w-full track as the chat's diff: without it a
                long line's tint stops at the viewport edge when scrolled. */}
            <div className="overflow-x-auto pb-1">
              <div className="w-max min-w-full font-mono text-[11px] leading-5">
                {file.lines.map((line, index) => (
                  <div
                    key={index}
                    className={`whitespace-pre px-3 ${
                      line.kind === 'removed'
                        ? 'bg-danger/15 text-text-secondary'
                        : line.kind === 'added'
                          ? 'bg-accent/15 text-text-secondary'
                          : line.kind === 'hunk'
                            ? 'bg-hover text-text-tertiary'
                            : 'text-text-secondary'
                    }`}
                  >
                    {line.text === '' ? ' ' : line.text}
                  </div>
                ))}
              </div>
            </div>
          </section>
        ))}
      </div>
    </div>
  );
}
