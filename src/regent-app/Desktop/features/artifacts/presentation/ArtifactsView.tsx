'use client';
// Artifacts — files a session saved under $REGENT_HOME/artifacts, one
// section per run (slug dir), newest first (artifacts.list). Search filters
// by file name; selecting a row loads its content via artifacts.get in the
// right pane. Master-detail split, mirroring Settings/cron conventions.
import { useMemo, useState } from 'react';
import { t } from '@/shared/i18n/t';
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { SearchField } from '@/shared/ui/SearchField';
import { useArtifactsList } from '@/features/artifacts/viewmodels/useArtifactsList';
import { ArtifactList } from '@/features/artifacts/presentation/ArtifactList';
import { ArtifactDetail } from '@/features/artifacts/presentation/ArtifactDetail';

export function ArtifactsView() {
  const s = t().artifacts;
  const { groups, loading, error } = useArtifactsList();
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState<string>();

  const selectedName = useMemo(() => {
    for (const group of groups) {
      const file = group.files.find((f) => f.rel === selected);
      if (file !== undefined) return file.name;
    }
    return undefined;
  }, [groups, selected]);

  return (
    <div className="flex h-full flex-col">
      <h1 className="shrink-0 px-4 pb-2 pt-4 text-lg font-semibold text-text-primary">{t().pages.artifacts}</h1>
      <div className="flex min-h-0 flex-1">
        <nav className="w-[260px] shrink-0 overflow-y-auto border-r border-stroke-tertiary p-2">
          <SearchField
            label={s.searchLabel}
            placeholder={s.searchPlaceholder}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            className="px-1.5"
          />
          {loading && (
            <div className="flex justify-center py-6">
              <Loader />
            </div>
          )}
          {error !== undefined && <ErrorState compact description={error} />}
          {!loading && error === undefined && (
            <ArtifactList groups={groups} query={query} selected={selected} onSelect={setSelected} />
          )}
        </nav>
        <div className="min-w-0 flex-1 overflow-y-auto">
          <ArtifactDetail rel={selected} name={selectedName} />
        </div>
      </div>
    </div>
  );
}
