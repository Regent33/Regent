'use client';
// Advanced — only the settings that map to a real config path get a control.
// Today that is cron.tick_interval_secs (the scheduler poll interval), bound
// through the generic ConfigField engine. No invented rows.
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { t } from '@/shared/i18n/t';
import { Section } from '@/features/settings/presentation/primitives';
import { ConfigField } from '@/features/settings/presentation/ConfigField';
import { useConfig } from '@/features/settings/viewmodels/useConfig';

export function AdvancedSection() {
  const s = t().settings.advanced;
  const cfg = useConfig();

  return (
    <Section title={s.title}>
      {cfg.loading && <Loader />}
      {cfg.error !== undefined && <ErrorState description={cfg.error} />}
      {!cfg.loading && cfg.error === undefined && (
        <>
          <ConfigField
            cfg={cfg}
            path="cron.tick_interval_secs"
            label={s.tickLabel}
            description={s.tickHint}
            applyLabel={s.apply}
            control={{ kind: 'number', min: 1, step: 1 }}
          />
          {cfg.writeError !== undefined && <ErrorState compact description={cfg.writeError} />}
          {cfg.note !== undefined && <p className="mt-2 text-xs text-text-tertiary">{cfg.note}</p>}
        </>
      )}
    </Section>
  );
}
