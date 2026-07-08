'use client';
// The Hermes "Main model" control: a Provider <select> + a Model <select>
// (a configured provider's models) OR a free-text model input (a bare kind or
// a configured provider with no listed models), with a centered Apply that
// arms only when the selection differs from the applied model. Apply maps to
// three config.set writes (see useModelConfig); its note/rejection renders
// verbatim below.
import { useEffect, useState } from 'react';
import { Button } from '@/shared/ui/Button';
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { t } from '@/shared/i18n/t';
import { SelectField, TextInput } from '@/features/settings/presentation/primitives';
import { useModelConfig } from '@/features/settings/viewmodels/useModelConfig';

export function MainModelPicker() {
  const s = t().settings.model;
  const vm = useModelConfig();
  const [provider, setProvider] = useState('');
  const [model, setModel] = useState('');

  // Seed the selection from the applied model once config lands (useState
  // can't read the async value at mount).
  useEffect(() => {
    if (vm.currentValue !== '' && provider === '') setProvider(vm.currentValue);
  }, [vm.currentValue, provider]);
  useEffect(() => {
    if (vm.currentModel !== undefined && model === '') setModel(vm.currentModel);
  }, [vm.currentModel, model]);

  if (vm.loading) return <Loader />;
  if (vm.error !== undefined) return <ErrorState description={vm.error} />;

  // A configured provider with a non-empty catalog gets a Model <select>;
  // anything else (bare kind, or configured-but-empty) gets a free-text field.
  const active = vm.configured.find((p) => `cfg:${p.name}` === provider);
  const modelOptions = active?.models ?? [];
  const freeText = modelOptions.length === 0;

  const onProvider = (next: string) => {
    setProvider(next);
    const nextActive = vm.configured.find((p) => `cfg:${p.name}` === next);
    const opts = nextActive?.models ?? [];
    // Keep the model when it stays valid; otherwise pick the first / clear.
    if (opts.length > 0 && !opts.includes(model)) setModel(opts[0]);
  };

  const armed =
    model.trim() !== '' && (provider !== vm.currentValue || model !== (vm.currentModel ?? ''));

  return (
    <div>
      <p className="text-xs text-text-tertiary">
        {s.currentMain}:{' '}
        {vm.currentModel !== undefined
          ? `${vm.currentProvider ?? ''} · ${vm.currentModel}`
          : s.currentUnknown}
      </p>
      <div className="mt-3 grid gap-2 sm:grid-cols-2">
        <SelectField
          label={s.providerLabel}
          value={provider}
          placeholder={s.selectProvider}
          options={vm.providerOptions}
          disabled={vm.applying}
          onChange={onProvider}
        />
        {freeText ? (
          <TextInput
            label={s.modelLabel}
            value={model}
            placeholder={s.freeModelPlaceholder}
            disabled={vm.applying}
            onChange={setModel}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && armed) vm.apply(provider, model);
            }}
          />
        ) : (
          <SelectField
            label={s.modelLabel}
            value={model}
            placeholder={s.selectModel}
            options={modelOptions.map((id) => ({ value: id, label: id }))}
            disabled={vm.applying}
            onChange={setModel}
          />
        )}
      </div>
      <div className="mt-3 flex justify-center">
        <Button
          size="sm"
          disabled={!armed || vm.applying}
          onClick={() => vm.apply(provider, model)}
        >
          {vm.applying ? s.applying : s.apply}
        </Button>
      </div>
      {vm.note !== undefined && <p className="mt-3 text-xs text-text-tertiary">{vm.note}</p>}
    </div>
  );
}
