'use client';
// Model section — the Hermes "Main model" layout (provider + model + a centered
// Apply, in MainModelPicker) over three config.set writes, plus a Context
// Window number field bound to context.max_tokens through the generic
// ConfigField engine. Auxiliary (per-task) models have no Regent backend, so we
// state that honestly instead of shipping dead Change rows.
import { t } from '@/shared/i18n/t';
import { ErrorState } from '@/shared/ui/ErrorState';
import { Section } from '@/features/settings/presentation/primitives';
import { ConfigField } from '@/features/settings/presentation/ConfigField';
import { MainModelPicker } from '@/features/settings/presentation/MainModelPicker';
import { MainModelsSection } from '@/features/settings/presentation/MainModelsSection';
import { useConfig } from '@/features/settings/viewmodels/useConfig';

export function ModelSection() {
  const s = t().settings.model;
  const cfg = useConfig();

  return (
    <Section title={s.title} description={s.description}>
      <MainModelPicker />

      <div className="mt-6 border-t border-stroke-tertiary pt-4">
        <MainModelsSection />
      </div>

      {!cfg.loading && cfg.error === undefined && (
        <div className="mt-6">
          <ConfigField
            cfg={cfg}
            path="context.max_tokens"
            label={s.contextWindowLabel}
            description={s.contextWindowHint}
            applyLabel={s.apply}
            control={{ kind: 'number', min: 1, step: 1000 }}
          />
          {cfg.writeError !== undefined && <ErrorState compact description={cfg.writeError} />}
          {cfg.note !== undefined && <p className="mt-2 text-xs text-text-tertiary">{cfg.note}</p>}
        </div>
      )}

      <div className="mt-6">
        <h3 className="text-sm font-semibold text-text-primary">{s.auxiliaryTitle}</h3>
        <p className="mt-1 text-xs text-text-tertiary">{s.auxiliaryNone}</p>
      </div>
    </Section>
  );
}
