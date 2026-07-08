'use client';
// API Keys section — one row per LLM provider key (env.list), each wired to
// env.set / env.unset via useApiKeys. Values are never displayed; only the
// deacon's masked preview. env errors render verbatim.
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { EmptyState } from '@/shared/ui/EmptyState';
import { t } from '@/shared/i18n/t';
import { Section } from '@/features/settings/presentation/primitives';
import { ApiKeyRow } from '@/features/settings/presentation/ApiKeyRow';
import { useApiKeys } from '@/features/settings/viewmodels/useApiKeys';

export function ApiKeysSection() {
  const s = t().settings.apiKeys;
  const vm = useApiKeys();

  return (
    <Section title={s.title}>
      <h3 className="text-sm font-semibold text-text-primary">{s.llmHeading}</h3>
      {vm.loading && (
        <div className="mt-2">
          <Loader />
        </div>
      )}
      {vm.error !== undefined && <ErrorState description={vm.error} />}
      {!vm.loading && vm.error === undefined && vm.keys.length === 0 && <EmptyState title={s.empty} />}
      {!vm.loading && vm.error === undefined && (
        <div className="mt-1">
          {vm.keys.map((entry) => (
            <ApiKeyRow
              key={entry.name}
              entry={entry}
              saving={vm.savingName === entry.name}
              onSave={vm.save}
              onRemove={vm.remove}
            />
          ))}
        </div>
      )}
    </Section>
  );
}
