'use client';
// Model section — current model (model.get) + a selectable catalog
// (model.list): built-ins plus configured providers' models. Picking a row
// calls model.set; the deacon's note ("applies to new sessions…") renders
// verbatim below the list.
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { ListRow } from '@/shared/ui/ListRow';
import { t } from '@/shared/i18n/t';
import { useModelSettings } from '@/features/settings/viewmodels/useModelSettings';

export function ModelSection() {
  const s = t().settings.model;
  const { current, options, loading, error, saving, note, setModel } = useModelSettings();

  return (
    <div className="p-6">
      <h2 className="text-lg font-semibold text-text-primary">{s.title}</h2>
      {loading && (
        <div className="mt-4">
          <Loader />
        </div>
      )}
      {error !== undefined && <ErrorState description={error} />}
      {!loading && error === undefined && (
        <>
          <p className="mt-1 text-xs text-text-tertiary">
            {s.current}: {current ?? s.currentUnknown}
          </p>
          <div className="mt-4">
            {options.map((option) => (
              <ListRow
                key={option.id}
                label={option.displayName}
                description={option.id}
                active={option.current}
                trailing={saving && option.id === current ? <Loader /> : undefined}
                onClick={() => setModel(option.id)}
              />
            ))}
          </div>
          {note !== undefined && <p className="mt-3 text-xs text-text-tertiary">{note}</p>}
        </>
      )}
    </div>
  );
}
