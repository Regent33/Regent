'use client';
// Fallbacks — the ordered chain chat falls to when the Main model (picked
// above in MainModelPicker) hits an outage (config agents_defaults.fallbacks).
// Each row picks a configured provider + one of its models; a {provider,model}
// already used by the main model or an earlier row is excluded from the
// options, and writes drop any duplicate that slips through — the same ref
// twice adds nothing to a fallback chain. Needs providers configured
// (API Keys + config.providers); empty state points there.
import { t } from '@/shared/i18n/t';
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { EmptyState } from '@/shared/ui/EmptyState';
import { Button } from '@/shared/ui/Button';
import { FieldRow, SelectField } from '@/features/settings/presentation/primitives';
import {
  type KeySlot,
  type ModelRef,
  type ProviderOption,
  useMainModels,
} from '@/features/settings/viewmodels/useMainModels';

const sameRef = (a: ModelRef | undefined, b: ModelRef | undefined) =>
  a !== undefined && b !== undefined && a.provider === b.provider && a.model === b.model;

function RefPicker({
  providers,
  value,
  onChange,
  taken,
  keySlots,
  onActivateKey,
  keyLabel,
}: {
  providers: readonly ProviderOption[];
  value?: ModelRef;
  onChange: (ref: ModelRef) => void;
  /** Refs already used elsewhere in the chain — hidden from the model options. */
  taken: readonly ModelRef[];
  /** Stored key slots for the selected provider — >1 shows the key picker. */
  keySlots?: readonly KeySlot[];
  onActivateKey?: (slot: number) => void;
  keyLabel?: string;
}) {
  const free = (provider: string, model: string) =>
    sameRef(value, { provider, model }) || !taken.some((u) => u.provider === provider && u.model === model);
  const models = (providers.find((p) => p.name === value?.provider)?.models ?? []).filter(
    (m) => value !== undefined && free(value.provider, m),
  );
  const pickProvider = (provider: string) => {
    const first = (providers.find((p) => p.name === provider)?.models ?? []).find((m) => free(provider, m)) ?? '';
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
      {keySlots !== undefined && keySlots.length > 1 && onActivateKey !== undefined && (
        <SelectField
          label={keyLabel ?? 'API key'}
          value="1"
          options={keySlots.map(({ slot, masked }) => ({
            value: String(slot),
            label: `${keyLabel ?? 'Key'} ${slot}${masked !== undefined ? ` ${masked}` : ''}`,
          }))}
          onChange={(next) => {
            const slot = Number(next);
            if (slot > 1) onActivateKey(slot);
          }}
        />
      )}
    </div>
  );
}

export function MainModelsSection() {
  const s = t().settings.mainModels;
  const vm = useMainModels();

  if (vm.loading) return <Loader />;
  if (vm.error !== undefined) return <ErrorState description={vm.error} />;
  if (vm.providers.length === 0) return <EmptyState title={s.needProviders} />;

  // Everything a row may not duplicate: the main model + the other rows.
  const takenFor = (i: number): readonly ModelRef[] =>
    [vm.primary, ...vm.fallbacks.filter((_, j) => j !== i)].filter((r): r is ModelRef => r !== undefined);
  const dedupe = (refs: readonly ModelRef[]) =>
    refs.filter((r, i) => !sameRef(r, vm.primary) && !refs.slice(0, i).some((p) => sameRef(p, r)));

  const setFallbackAt = (i: number, ref: ModelRef) =>
    vm.setFallbacks(dedupe(vm.fallbacks.map((f, j) => (j === i ? ref : f))));
  const addFallback = () => {
    const used = takenFor(-1);
    for (const p of vm.providers) {
      const m = p.models.find((model) => !used.some((u) => u.provider === p.name && u.model === model));
      if (m !== undefined) {
        vm.setFallbacks([...vm.fallbacks, { provider: p.name, model: m }]);
        return;
      }
    }
  };
  const removeFallback = (i: number) => vm.setFallbacks(vm.fallbacks.filter((_, j) => j !== i));

  return (
    <div>
      <h3 className="text-sm font-semibold text-text-primary">{s.title}</h3>
      <p className="mt-0.5 text-xs text-text-tertiary">{s.description}</p>

      {vm.fallbacks.map((f, i) => (
        <FieldRow
          key={i}
          label={i === 0 ? s.secondary : `${s.fallback} ${i}`}
          control={
            <div className="flex items-center gap-1.5">
              <RefPicker
                providers={vm.providers}
                value={f}
                onChange={(ref) => setFallbackAt(i, ref)}
                taken={takenFor(i)}
                keySlots={vm.keySlotsFor(f.provider)}
                onActivateKey={(slot) => vm.activateKey(f.provider, slot)}
                keyLabel={s.keyLabel}
              />
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
