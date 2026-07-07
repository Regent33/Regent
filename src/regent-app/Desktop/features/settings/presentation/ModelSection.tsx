'use client';
// Model section — current model (model.get) + a selectable catalog
// (model.list): built-ins plus configured providers' models. Picking a row
// calls model.set; the deacon's note ("applies to new sessions…") renders
// verbatim below the list.
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { ListRow } from '@/shared/ui/ListRow';
import { t } from '@/shared/i18n/t';
import { Section } from '@/features/settings/presentation/primitives';
import { useModelSettings } from '@/features/settings/viewmodels/useModelSettings';

export function ModelSection() {
  const s = t().settings.model;
  const { current, options, loading, error, saving, note, setModel } = useModelSettings();

  return (
    <Section title={s.title} description={`${s.current}: ${current ?? s.currentUnknown}`}>
      {loading && <Loader />}
      {error !== undefined && <ErrorState description={error} />}
      {!loading && error === undefined && (
        <>
          <div>
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
    </Section>
  );
}
