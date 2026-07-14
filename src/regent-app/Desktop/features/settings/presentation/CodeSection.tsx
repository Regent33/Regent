'use client';
// Code — code-related toggles that map to real config paths. Today that is
// tools.auto_approve (the deacon's live approval policy): config.set applies
// it instantly to open sessions. Bound through the generic ConfigField engine.
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { t } from '@/shared/i18n/t';
import { Section } from '@/features/settings/presentation/primitives';
import { ConfigField } from '@/features/settings/presentation/ConfigField';
import { useConfig } from '@/features/settings/viewmodels/useConfig';

export function CodeSection() {
  const s = t().settings.code;
  const cfg = useConfig();

  return (
    <Section title={s.title}>
      {cfg.loading && <Loader />}
      {cfg.error !== undefined && <ErrorState description={cfg.error} />}
      {!cfg.loading && cfg.error === undefined && (
        <>
          <ConfigField
            cfg={cfg}
            path="tools.auto_approve"
            label={s.autoApproveLabel}
            description={s.autoApproveHint}
            applyLabel={s.apply}
            control={{ kind: 'toggle' }}
          />
          {cfg.writeError !== undefined && <ErrorState compact description={cfg.writeError} />}
          {cfg.note !== undefined && <p className="mt-2 text-xs text-text-tertiary">{cfg.note}</p>}
        </>
      )}
    </Section>
  );
}
