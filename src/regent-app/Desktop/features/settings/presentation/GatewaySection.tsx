'use client';
// Gateway — read-only presence view. DeaconConfig has no platform/gateway
// section at all (webhook adapters register purely from environment secrets,
// per ADR-015/webhook/registry.rs), so there is nothing for config.get to
// show here. The one real, already-wired signal is env.list's "messaging"
// group (also feeds the API Keys page) — which platform credentials are
// present. Reused read-only (no save/remove here; that's API Keys' job).
// The gateway process itself is entirely CLI-managed (ADR-015): no RPC
// starts/stops it or lists which platforms are actually live.
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { EmptyState } from '@/shared/ui/EmptyState';
import { t } from '@/shared/i18n/t';
import { Section, FieldRow } from '@/features/settings/presentation/primitives';
import { useApiKeys } from '@/features/settings/viewmodels/useApiKeys';

export function GatewaySection() {
  const s = t().settings.gateway;
  const k = t().settings.apiKeys;
  const { keys, loading, error } = useApiKeys();
  const platforms = keys.filter((key) => key.group === 'messaging');

  return (
    <Section title={s.title} description={s.description}>
      {loading && <Loader />}
      {error !== undefined && <ErrorState description={error} />}
      {!loading && error === undefined && platforms.length === 0 && <EmptyState title={s.empty} />}
      {!loading &&
        error === undefined &&
        platforms.map((key) => (
          <FieldRow
            key={key.name}
            label={key.label}
            control={
              <p className={`text-sm sm:text-right ${key.set ? 'text-text-primary' : 'text-text-tertiary'}`}>
                {key.set ? (key.masked ?? k.set) : k.unset}
              </p>
            }
          />
        ))}
      <p className="mt-3 text-xs text-text-tertiary">{s.note}</p>
    </Section>
  );
}
