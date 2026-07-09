'use client';
// Chat — the per-turn behavior knobs, bound through the same ConfigField
// engine (config.get/config.set) and the SAME dotted paths as Memory &
// Context (context.trigger_fraction, context.protect_last_n,
// limits.max_turn_tokens). Reuses that section's i18n labels on purpose —
// it's the identical live value, just surfaced where a user looks for
// "how chat behaves" rather than "what's remembered".
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { t } from '@/shared/i18n/t';
import { Section } from '@/features/settings/presentation/primitives';
import { ConfigField } from '@/features/settings/presentation/ConfigField';
import { useConfig } from '@/features/settings/viewmodels/useConfig';

export function ChatSection() {
  const s = t().settings.chat;
  const m = t().settings.memory;
  const cfg = useConfig();

  return (
    <Section title={s.title} description={s.description}>
      {cfg.loading && <Loader />}
      {cfg.error !== undefined && <ErrorState description={cfg.error} />}
      {!cfg.loading && cfg.error === undefined && (
        <>
          <ConfigField
            cfg={cfg}
            path="context.trigger_fraction"
            label={m.triggerLabel}
            description={m.triggerHint}
            applyLabel={m.apply}
            control={{ kind: 'number', min: 0, max: 1, step: 0.05 }}
          />
          <ConfigField
            cfg={cfg}
            path="context.protect_last_n"
            label={m.protectLabel}
            description={m.protectHint}
            applyLabel={m.apply}
            control={{ kind: 'number', min: 0, step: 1 }}
          />
          <ConfigField
            cfg={cfg}
            path="limits.max_turn_tokens"
            label={m.maxTurnLabel}
            description={m.maxTurnHint}
            applyLabel={m.apply}
            control={{ kind: 'number', min: 1, step: 1000 }}
          />
          {cfg.writeError !== undefined && <ErrorState compact description={cfg.writeError} />}
          {cfg.note !== undefined && <p className="mt-2 text-xs text-text-tertiary">{cfg.note}</p>}
        </>
      )}
    </Section>
  );
}
