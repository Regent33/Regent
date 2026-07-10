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

/** The slot numbers a provider can ride: its stored key slots, or the implicit
 *  base slot for keyless providers (local ollama etc.) so they stay pickable. */
const slotNumbers = (slots: readonly KeySlot[]): readonly number[] =>
  slots.length > 0 ? slots.map((s) => s.slot) : [1];

function RefPicker({
  providers,
  value,
  onChange,
  taken,
  slotsFor,
  keyLabel,
}: {
  providers: readonly ProviderOption[];
  value?: ModelRef;
  onChange: (ref: ModelRef) => void;
  /** Refs already used elsewhere in the chain — hidden from the model options. */
  taken: readonly ModelRef[];
  /** Stored key slots per provider (vm.keySlotsFor). */
  slotsFor: (provider: string) => readonly KeySlot[];
  keyLabel: string;
}) {
  const m = t().settings.model;
  // Key exhaustion: a provider+model combo supports as many chain links as the
  // provider has stored keys. A model stays pickable while ANY slot is free;
  // picking it lands on the row's own slot when free, else the first free one.
  const usedSlots = (provider: string, model: string): readonly number[] =>
    taken.filter((u) => u.provider === provider && u.model === model).map(slotOf);
  const freeSlot = (provider: string, model: string, prefer?: number): number | undefined => {
    const used = usedSlots(provider, model);
    const all = slotNumbers(slotsFor(provider));
    if (prefer !== undefined && all.includes(prefer) && !used.includes(prefer)) return prefer;
    return all.find((n) => !used.includes(n));
  };
  const catalog = providers.find((p) => p.name === value?.provider)?.models ?? [];
  const models = catalog.filter(
    (mo) => value !== undefined && (mo === value.model || freeSlot(value.provider, mo) !== undefined),
  );
  const pickModel = (provider: string, model: string) => {
    onChange(withKeySlot({ provider, model }, freeSlot(provider, model, value?.key_slot) ?? 1));
  };

  // Local draft for the free-text field — commits on blur/Enter so typing
  // doesn't fire a config.set per keystroke.
  const [draft, setDraft] = useState(value?.model ?? '');
  useEffect(() => {
    setDraft(value?.model ?? '');
  }, [value?.provider, value?.model]);
  const commitDraft = () => {
    const trimmed = draft.trim();
    if (value !== undefined && trimmed !== '' && trimmed !== value.model) {
      pickModel(value.provider, trimmed);
    }
  };

  const pickProvider = (provider: string) => {
    const first =
      (providers.find((p) => p.name === provider)?.models ?? []).find(
        (mo) => freeSlot(provider, mo) !== undefined,
      ) ?? '';
    pickModel(provider, first);
  };

  // The row's key options: its own slot plus every slot not spent on the same
  // provider+model by another link. The own slot is guaranteed present (even
  // against a stale env.list) so the select never shows blank.
  const providerSlots = value === undefined ? [] : slotsFor(value.provider);
  const filtered =
    value === undefined
      ? []
      : providerSlots.filter(
          (s) => s.slot === slotOf(value) || !usedSlots(value.provider, value.model).includes(s.slot),
        );
  const rowKeySlots =
    value !== undefined && !filtered.some((s) => s.slot === slotOf(value))
      ? [{ slot: slotOf(value) }, ...filtered]
      : filtered;

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
          onChange={(mo) => value !== undefined && pickModel(value.provider, mo)}
        />
      )}
      {providerSlots.length > 1 && (
        <KeyPickerField
          slots={rowKeySlots}
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

  // What "+ Add fallback" auto-picks. DISTINCT (provider, model) pairs first —
  // filling key slots before moving to the next model produced repetitive
  // chains (same model on Key 1 then Key 2, then the next model twice, …).
  // Riding a second key on an already-used model stays possible, but only as
  // the last resort once every catalog model is in the chain; a fully-spent
  // catalog disables the button instead of writing a duplicate. Catalog-less
  // providers contribute '' (free-text) rows.
  const nextAvailable = (used: readonly ModelRef[]): ModelRef | undefined => {
    // Pass 1: a provider+model no link (or the main model) uses in ANY slot.
    for (const p of vm.providers) {
      const models = p.models.length > 0 ? p.models : [''];
      const slots = slotNumbers(vm.keySlotsFor(p.name));
      for (const mo of models) {
        if (!used.some((u) => u.provider === p.name && u.model === mo)) {
          return withKeySlot({ provider: p.name, model: mo }, slots[0] ?? 1);
        }
      }
    }
    // Pass 2: everything is used somewhere — take the first free key slot.
    for (const p of vm.providers) {
      const models = p.models.length > 0 ? p.models : [''];
      const slots = slotNumbers(vm.keySlotsFor(p.name));
      for (const mo of models) {
        const n = slots.find(
          (slot) => !used.some((u) => u.provider === p.name && u.model === mo && slotOf(u) === slot),
        );
        if (n !== undefined) return withKeySlot({ provider: p.name, model: mo }, n);
      }
    }
    return undefined;
  };
  const nextRef = nextAvailable(takenFor(-1));
  const addFallback = () => {
    if (nextRef !== undefined) vm.setFallbacks([...vm.fallbacks, nextRef]);
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
                slotsFor={vm.keySlotsFor}
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
        <Button variant="ghost" size="sm" disabled={nextRef === undefined} onClick={addFallback}>
          {s.addFallback}
        </Button>
      </div>
      {vm.note !== undefined && <p className="mt-2 text-xs text-text-tertiary">{vm.note}</p>}
    </div>
  );
}
