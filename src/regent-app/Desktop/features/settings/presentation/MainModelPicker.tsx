'use client';
// The Hermes "Main model" control: a Provider <select> + a Model <select>
// (a configured provider's models) OR a free-text model input (a configured
// provider with no listed models), a Key picker when the provider holds more
// than one stored key, and a centered Apply that arms only when the selection
// differs from the applied model. Writes agents_defaults.primary — the SAME
// canonical value the deacon resolves chat through and the Fallback rows below
// read to exclude duplicates (see useMainModels; `vm` is shared with
// MainModelsSection so both stay in sync).
import { useEffect, useState } from 'react';
import { Button } from '@/shared/ui/Button';
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { t } from '@/shared/i18n/t';
import { SelectField, TextInput } from '@/features/settings/presentation/primitives';
import { KeyPickerField } from '@/features/settings/presentation/KeyPickerField';
import { type MainModelsState, withKeySlot } from '@/features/settings/viewmodels/useMainModels';

export function MainModelPicker({ vm }: { vm: MainModelsState }) {
  const s = t().settings.model;
  const keyLabel = t().settings.mainModels.keyLabel;
  const [provider, setProvider] = useState('');
  const [model, setModel] = useState('');
  // The pending key slot (1 = base key); part of the selection, applied with it.
  const [slot, setSlot] = useState(1);

  // Seed the selection from the applied primary once config lands (useState
  // can't read the async value at mount).
  useEffect(() => {
    if (vm.primary !== undefined && provider === '') {
      setProvider(vm.primary.provider);
      setSlot(vm.primary.key_slot ?? 1);
    }
  }, [vm.primary, provider]);
  useEffect(() => {
    if (vm.primary !== undefined && model === '') setModel(vm.primary.model);
  }, [vm.primary, model]);

  if (vm.loading) return <Loader />;
  if (vm.error !== undefined) return <ErrorState description={vm.error} />;
  // No configured provider yet — nothing valid to pick; MainModelsSection's
  // EmptyState right below already points at API Keys.
  if (vm.providers.length === 0) return null;

  // A configured provider with a non-empty catalog gets a Model <select>;
  // a configured provider with an empty catalog gets a free-text field.
  const active = vm.providers.find((p) => p.name === provider);
  const modelOptions = active?.models ?? [];
  const freeText = provider !== '' && modelOptions.length === 0;

  const onProvider = (next: string) => {
    setProvider(next);
    setSlot(1); // another provider's slots — start from its base key
    const opts = vm.providers.find((p) => p.name === next)?.models ?? [];
    // Keep the model when it stays valid; otherwise pick the first / clear.
    if (opts.length > 0 && !opts.includes(model)) setModel(opts[0]);
  };

  const armed =
    model.trim() !== '' &&
    (provider !== (vm.primary?.provider ?? '') ||
      model !== (vm.primary?.model ?? '') ||
      slot !== (vm.primary?.key_slot ?? 1));

  const apply = () => vm.setPrimary(withKeySlot({ provider, model: model.trim() }, slot));

  return (
    <div>
      <p className="text-xs text-text-tertiary">
        {s.currentMain}:{' '}
        {vm.primary !== undefined ? `${vm.primary.provider} · ${vm.primary.model}` : s.currentUnknown}
      </p>
      {/* One row, matching the fallback rows: Provider · Model · Key · Apply. */}
      <div className="mt-3 flex items-center gap-1.5">
        <SelectField
          label={s.providerLabel}
          value={provider}
          placeholder={s.selectProvider}
          options={vm.providers.map((p) => ({ value: p.name, label: p.name }))}
          onChange={onProvider}
        />
        {freeText ? (
          <TextInput
            label={s.modelLabel}
            value={model}
            placeholder={s.freeModelPlaceholder}
            onChange={setModel}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && armed) apply();
            }}
          />
        ) : (
          <SelectField
            label={s.modelLabel}
            value={model}
            placeholder={s.selectModel}
            options={modelOptions.map((id) => ({ value: id, label: id }))}
            onChange={setModel}
          />
        )}
        {provider !== '' && (
          <KeyPickerField
            slots={vm.keySlotsFor(provider)}
            value={slot}
            onSelect={setSlot}
            label={keyLabel}
          />
        )}
        <Button size="sm" disabled={!armed} onClick={apply}>
          {s.apply}
        </Button>
      </div>
      {vm.note !== undefined && <p className="mt-3 text-xs text-text-tertiary">{vm.note}</p>}
    </div>
  );
}
