'use client';
// Safety — the approval policy and the sandbox note. Auto mode
// (tools.auto_approve) lived on the Code page, which is where nobody looked
// for "approve everything": it governs every tool gate, not just the coding
// harness. The tool jail is REGENT_SANDBOX (an env var, not a config field),
// so that stays an explanatory note instead of a dead toggle.
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { t } from '@/shared/i18n/t';
import { Section } from '@/features/settings/presentation/primitives';
import { ConfigField } from '@/features/settings/presentation/ConfigField';
import { useConfig } from '@/features/settings/viewmodels/useConfig';

export function SafetySection() {
  const s = t().settings.safety;
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
      <h3 className="mt-6 text-sm font-semibold text-text-primary">{s.sandboxTitle}</h3>
      <p className="mt-1 text-xs text-text-tertiary">{s.sandboxText}</p>
    </Section>
  );
}
