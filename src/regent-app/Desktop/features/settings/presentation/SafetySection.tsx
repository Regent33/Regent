'use client';
// Safety — honestly read-only. There is no approval-policy or sandbox field
// anywhere in DeaconConfig (checked domain/config/*.rs): approval routes
// per-session over RPC unless REGENT_AUTO_APPROVE is set in the environment,
// and the tool jail is REGENT_SANDBOX (also env, also not in config.yaml).
// Neither is settable from the app today, so this page states the real
// behavior instead of drawing dead toggles.
import { t } from '@/shared/i18n/t';
import { Section } from '@/features/settings/presentation/primitives';

export function SafetySection() {
  const s = t().settings.safety;

  return (
    <Section title={s.title}>
      <h3 className="text-sm font-semibold text-text-primary">{s.approvalTitle}</h3>
      <p className="mt-1 text-xs text-text-tertiary">{s.approvalText}</p>

      <h3 className="mt-5 text-sm font-semibold text-text-primary">{s.sandboxTitle}</h3>
      <p className="mt-1 text-xs text-text-tertiary">{s.sandboxText}</p>
    </Section>
  );
}
