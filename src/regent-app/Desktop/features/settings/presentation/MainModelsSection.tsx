'use client';
// Main models — the Primary → Secondary → Fallbacks chain chat runs through
// (config agents_defaults). Each row picks a configured provider + one of its
// models; the deacon tries the primary first every turn and reroutes to the
// next on a transport/5xx/rate-limit/auth failure (never a 4xx), returning to
// the primary the moment it recovers. Needs providers configured (API Keys +
// config.providers); empty state points there.
import { t } from '@/shared/i18n/t';
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { EmptyState } from '@/shared/ui/EmptyState';
import { Button } from '@/shared/ui/Button';
import { FieldRow, SelectField } from '@/features/settings/presentation/primitives';
import {
  type ModelRef,
  type ProviderOption,
  useMainModels,
} from '@/features/settings/viewmodels/useMainModels';

function RefPicker({
  providers,
  value,
  onChange,
}: {
  providers: readonly ProviderOption[];
  value?: ModelRef;
  onChange: (ref: ModelRef) => void;
}) {
  const models = providers.find((p) => p.name === value?.provider)?.models ?? [];
  const pickProvider = (provider: string) => {
    const first = providers.find((p) => p.name === provider)?.models[0] ?? '';
    onChange({ provider, model: first });
  };
  return (
    <div className="flex gap-1.5">
      <SelectField
        label="Provider"
        value={value?.provider ?? ''}
        placeholder="Provider"
        options={providers.map((p) => ({ value: p.name, label: p.name }))}
        onChange={pickProvider}
      />
      <SelectField
        label="Model"
        value={value?.model ?? ''}
        placeholder="Model"
        options={models.map((m) => ({ value: m, label: m }))}
        onChange={(model) => value !== undefined && onChange({ ...value, model })}
      />
    </div>
  );
}

export function MainModelsSection() {
  const s = t().settings.mainModels;
  const vm = useMainModels();

  if (vm.loading) return <Loader />;
  if (vm.error !== undefined) return <ErrorState description={vm.error} />;
  if (vm.providers.length === 0) return <EmptyState title={s.needProviders} />;

  const setFallbackAt = (i: number, ref: ModelRef) =>
    vm.setFallbacks(vm.fallbacks.map((f, j) => (j === i ? ref : f)));
  const addFallback = () =>
    vm.setFallbacks([...vm.fallbacks, { provider: vm.providers[0].name, model: vm.providers[0].models[0] ?? '' }]);
  const removeFallback = (i: number) => vm.setFallbacks(vm.fallbacks.filter((_, j) => j !== i));

  return (
    <div>
      <h3 className="text-sm font-semibold text-text-primary">{s.title}</h3>
      <p className="mt-0.5 text-xs text-text-tertiary">{s.description}</p>

      <FieldRow
        label={s.primary}
        description={s.primaryHint}
        control={<RefPicker providers={vm.providers} value={vm.primary} onChange={vm.setPrimary} />}
      />

      {vm.fallbacks.map((f, i) => (
        <FieldRow
          key={i}
          label={i === 0 ? s.secondary : `${s.fallback} ${i}`}
          control={
            <div className="flex items-center gap-1.5">
              <RefPicker providers={vm.providers} value={f} onChange={(ref) => setFallbackAt(i, ref)} />
              <Button variant="ghost" size="sm" aria-label={s.remove} onClick={() => removeFallback(i)}>
                ×
              </Button>
            </div>
          }
        />
      ))}

      <div className="mt-3">
        <Button variant="ghost" size="sm" onClick={addFallback}>
          {s.addFallback}
        </Button>
      </div>
      {vm.note !== undefined && <p className="mt-2 text-xs text-text-tertiary">{vm.note}</p>}
    </div>
  );
}
