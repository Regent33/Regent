'use client';
// Safety — sandbox-only, honestly read-only. The tool jail is REGENT_SANDBOX
// (an env var, not a config field), so it stays an explanatory note instead of
// a dead toggle. The approval policy (tools.auto_approve) is a live toggle now,
// but it lives on the dedicated Code page — see CodeSection.tsx.
import { t } from '@/shared/i18n/t';
import { Section } from '@/features/settings/presentation/primitives';

export function SafetySection() {
  const s = t().settings.safety;

  return (
    <Section title={s.title}>
      <h3 className="text-sm font-semibold text-text-primary">{s.sandboxTitle}</h3>
      <p className="mt-1 text-xs text-text-tertiary">{s.sandboxText}</p>
    </Section>
  );
}
