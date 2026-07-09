'use client';
// Gateway — the platform credentials, editable in place. DeaconConfig has no
// platform/gateway section (webhook adapters register purely from environment
// secrets, per ADR-015/webhook/registry.rs), so the real, already-wired
// surface is env.list's "messaging" group: the same rows the API Keys page
// manages, offered here too so setting up a platform doesn't bounce between
// pages (same env.set/env.unset writes, masked values only). The gateway
// process itself is entirely CLI-managed (ADR-015): no RPC starts/stops it or
// lists which platforms are actually live.
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { EmptyState } from '@/shared/ui/EmptyState';
import { t } from '@/shared/i18n/t';
import { Section } from '@/features/settings/presentation/primitives';
import { ApiKeyRow } from '@/features/settings/presentation/ApiKeyRow';
import { useApiKeys } from '@/features/settings/viewmodels/useApiKeys';

export function GatewaySection() {
  const s = t().settings.gateway;
  const vm = useApiKeys();
  // Base rows only — numbered multi-key slots stay an API Keys page affair.
  const platforms = vm.keys.filter((key) => key.group === 'messaging' && !/_\d+$/.test(key.name));

  return (
    <Section title={s.title} description={s.description}>
      {vm.loading && <Loader />}
      {vm.error !== undefined && <ErrorState description={vm.error} />}
      {!vm.loading && vm.error === undefined && platforms.length === 0 && <EmptyState title={s.empty} />}
      {!vm.loading &&
        vm.error === undefined &&
        platforms.map((key) => (
          <ApiKeyRow
            key={key.name}
            entry={key}
            saving={vm.savingName === key.name}
            onSave={vm.save}
            onRemove={vm.remove}
          />
        ))}
      <p className="mt-3 text-xs text-text-tertiary">{s.note}</p>
    </Section>
  );
}
