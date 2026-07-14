'use client';
// Workspace — memory.embeddings is a real config field, so it binds through
// the generic ConfigField engine (config.set: validated, verbatim errors).
// Memory home is intentionally NOT editable: the deacon resolves its data
// directory from REGENT_HOME *before* config.yaml is read (the file lives
// inside that directory), so `memory.home` can never take effect — we show
// the honest way to move it instead of a control that would silently no-op.
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { t } from '@/shared/i18n/t';
import { Section, FieldRow } from '@/features/settings/presentation/primitives';
import { ConfigField } from '@/features/settings/presentation/ConfigField';
import { useConfig } from '@/features/settings/viewmodels/useConfig';

export function WorkspaceSection() {
  const s = t().settings.workspace;
  const cfg = useConfig();
  const home = cfg.get('memory.home');

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
          <ConfigField
            cfg={cfg}
            path="memory.embeddings"
            label={s.embeddingsLabel}
            description={s.embeddingsHint}
            applyLabel={s.apply}
            control={{ kind: 'toggle' }}
          />
          {cfg.writeError !== undefined && <ErrorState compact description={cfg.writeError} />}
          {cfg.note !== undefined && <p className="mt-2 text-xs text-text-tertiary">{cfg.note}</p>}
          <p className="mt-3 text-xs text-text-tertiary">{s.note}</p>
        </>
      )}
    </Section>
  );
}
