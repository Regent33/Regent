'use client';
// MCP — honest empty state. DeaconConfig has no `mcp` section at all (grepped
// domain/config/*.rs): `regent mcp serve` exposes REGENT's own tools TO an
// external MCP client over stdio, but nothing lets the deacon consume
// external MCP servers, so config.get can never return a server list here.
// Nothing to bind — this is the true, current state, not a stub.
import { t } from '@/shared/i18n/t';
import { EmptyState } from '@/shared/ui/EmptyState';
import { Section } from '@/features/settings/presentation/primitives';

export function McpSection() {
  const s = t().settings.mcp;

  return (
    <Section title={s.title}>
      <EmptyState title={s.empty} hint={s.hint} />
    </Section>
  );
}
