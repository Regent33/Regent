'use client';
// Workspace — read-only info from config.get's memory section (the only
// workspace-shaped data the deacon actually exposes: `memory.home` and
// `memory.embeddings`). Checked status.get too: it returns model/
// active_sessions/cron, nothing workspace-related, and there is no `cwd` or
// sandbox-flag RPC anywhere (the tool jail is REGENT_SANDBOX, an env var, not
// a config field) — so those stay an honest note instead of invented rows.
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { t } from '@/shared/i18n/t';
import { Section, FieldRow } from '@/features/settings/presentation/primitives';
import { useConfig } from '@/features/settings/viewmodels/useConfig';

export function WorkspaceSection() {
  const s = t().settings.workspace;
  const cfg = useConfig();
  const home = cfg.get('memory.home');
  const embeddings = cfg.get('memory.embeddings');

  return (
    <Section title={s.title} description={s.description}>
      {cfg.loading && <Loader />}
      {cfg.error !== undefined && <ErrorState description={cfg.error} />}
      {!cfg.loading && cfg.error === undefined && (
        <>
          <FieldRow
            label={s.homeLabel}
            description={s.homeHint}
            control={<p className="text-sm text-text-primary sm:text-right">{typeof home === 'string' ? home : s.unknown}</p>}
          />
          <FieldRow
            label={s.embeddingsLabel}
            description={s.embeddingsHint}
            control={
              <p className="text-sm text-text-primary sm:text-right">
                {embeddings === true ? s.embeddingsOn : embeddings === false ? s.embeddingsOff : s.unknown}
              </p>
            }
          />
          <p className="mt-3 text-xs text-text-tertiary">{s.note}</p>
        </>
      )}
    </Section>
  );
}
