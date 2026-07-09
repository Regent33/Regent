'use client';
// Fallbacks — the ordered chain chat falls to when the Main model (picked
// above in MainModelPicker) hits an outage (config agents_defaults.fallbacks).
// Each row picks a configured provider + one of its models — a free-text
// field when that provider's catalog is empty; a {provider,model} already
// used by the main model or an earlier row is excluded from the options, and
// writes drop any duplicate that slips through — the same ref twice adds
// nothing to a fallback chain. `vm` is shared with MainModelPicker (see
// ModelSection) so a Main model change is reflected here without a refetch.
// Needs providers configured (API Keys + config.providers); empty state
// points there.
import { useEffect, useState } from 'react';
import { t } from '@/shared/i18n/t';
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { EmptyState } from '@/shared/ui/EmptyState';
import { Button } from '@/shared/ui/Button';
import { FieldRow, SelectField, TextInput } from '@/features/settings/presentation/primitives';
import { KeyPickerField } from '@/features/settings/presentation/KeyPickerField';
import {
  type KeySlot,
  type MainModelsState,
  type ModelRef,
  type ProviderOption,
  withKeySlot,
} from '@/features/settings/viewmodels/useMainModels';

// Identity is the (provider, model, key slot) TRIPLE — the same model pinned
// to a different stored key is a distinct, legitimate chain link (multi-key
// failover). Slot 1 and "no slot" are the same base key.
const slotOf = (r: ModelRef) => r.key_slot ?? 1;
const sameRef = (a: ModelRef | undefined, b: ModelRef | undefined) =>
  a !== undefined &&
  b !== undefined &&
  a.provider === b.provider &&
  a.model === b.model &&
  slotOf(a) === slotOf(b);

function RefPicker({
  providers,
  value,
  onChange,
  taken,
  keySlots,
  keyLabel,
}: {
  providers: readonly ProviderOption[];
  value?: ModelRef;
  onChange: (ref: ModelRef) => void;
  /** Refs already used elsewhere in the chain — hidden from the model options. */
  taken: readonly ModelRef[];
  /** Stored key slots for the selected provider — >1 shows the key picker. */
  keySlots?: readonly KeySlot[];
  keyLabel: string;
}) {
  const m = t().settings.model;
  const free = (provider: string, model: string) => {
    const candidate: ModelRef = { provider, model, key_slot: value?.key_slot };
    return sameRef(value, candidate) || !taken.some((u) => sameRef(u, candidate));
  };
  const catalog = providers.find((p) => p.name === value?.provider)?.models ?? [];
  const models = catalog.filter((mo) => value !== undefined && free(value.provider, mo));

  // Local draft for the free-text field — commits on blur/Enter so typing
  // doesn't fire a config.set per keystroke.
  const [draft, setDraft] = useState(value?.model ?? '');
  useEffect(() => {
    setDraft(value?.model ?? '');
  }, [value?.provider, value?.model]);
  const commitDraft = () => {
    const trimmed = draft.trim();
    if (value !== undefined && trimmed !== '' && trimmed !== value.model) {
      onChange({ ...value, model: trimmed });
    }
  };

  const pickProvider = (provider: string) => {
    // Another provider's slots — the ref restarts on its base key.
    const first = (providers.find((p) => p.name === provider)?.models ?? []).find((mo) => free(provider, mo)) ?? '';
    onChange({ provider, model: first });
  };

  return (
    <div className="flex gap-1.5">
      <SelectField
        label={m.providerLabel}
        value={value?.provider ?? ''}
        placeholder={m.selectProvider}
        options={providers.map((p) => ({ value: p.name, label: p.name }))}
        onChange={pickProvider}
      />
      {catalog.length === 0 ? (
        // Provider without a listed catalog — type the model id (same free-text
        // escape the Main model picker has); never an empty, unusable select.
        <TextInput
          label={m.modelLabel}
          value={draft}
          placeholder={m.freeModelPlaceholder}
          onChange={setDraft}
          onBlur={commitDraft}
          onKeyDown={(e) => {
            if (e.key === 'Enter') commitDraft();
          }}
        />
      ) : (
        <SelectField
          label={m.modelLabel}
          value={value?.model ?? ''}
          placeholder={m.selectModel}
          options={models.map((mo) => ({ value: mo, label: mo }))}
          onChange={(mo) => value !== undefined && onChange({ ...value, model: mo })}
        />
      )}
      {keySlots !== undefined && (
        <KeyPickerField
          slots={keySlots}
          value={value?.key_slot}
          onSelect={(slot) => value !== undefined && onChange(withKeySlot(value, slot))}
          label={keyLabel}
        />
      )}
    </div>
  );
}

export function MainModelsSection({ vm }: { vm: MainModelsState }) {
  const s = t().settings.mainModels;

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

  // First not-yet-used {provider,model} — catalog providers pick a free listed
  // model, catalog-less providers pick '' (free-text) once, so the empty slot
  // itself never gets added twice.
  const nextAvailable = (used: readonly ModelRef[]): ModelRef | undefined => {
    for (const p of vm.providers) {
      if (p.models.length === 0) {
        if (!used.some((u) => u.provider === p.name && u.model === '')) return { provider: p.name, model: '' };
        continue;
      }
      const m = p.models.find((model) => !used.some((u) => u.provider === p.name && u.model === model));
      if (m !== undefined) return { provider: p.name, model: m };
    }
    return undefined;
  };
  const addFallback = () => {
    const ref = nextAvailable(takenFor(-1)) ?? { provider: vm.providers[0].name, model: '' };
    vm.setFallbacks([...vm.fallbacks, ref]);
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
