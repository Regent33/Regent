'use client';
// Optional vision route. Provider names come from the same configured-provider
// catalog as chat, so local/keyless and hosted entries stay distinct without a
// second discovery path. One Apply writes the whole speech.vision object,
// preserving input_mode/download_timeout.
import { useEffect, useState } from 'react';
import { Button } from '@/shared/ui/Button';
import { t } from '@/shared/i18n/t';
import { SelectField, TextInput } from '@/features/settings/presentation/primitives';
import type { ConfigState } from '@/features/settings/viewmodels/useConfig';
import type { MainModelsState } from '@/features/settings/viewmodels/useMainModels';

const AUTO = 'auto';
const CUSTOM = '__custom__';

function objectValue(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : {};
}

export function VisionModelSection({ cfg, vm }: { cfg: ConfigState; vm: MainModelsState }) {
  const s = t().settings.model;
  const current = objectValue(cfg.get('speech.vision'));
  const appliedProvider =
    typeof current.provider === 'string' && current.provider.trim() !== '' ? current.provider : AUTO;
  const appliedModel = typeof current.model === 'string' ? current.model : '';
  const [provider, setProvider] = useState('');
  const [model, setModel] = useState('');
  const [custom, setCustom] = useState(false);

  useEffect(() => {
    if (cfg.loading || provider !== '') return;
    setProvider(appliedProvider);
    setModel(appliedModel);
  }, [appliedModel, appliedProvider, cfg.loading, provider]);

  const active = vm.providers.find((entry) => entry.name === provider);
  const models = active?.models ?? [];
  const explicit = provider !== '' && provider !== AUTO;
  const freeText = explicit && (custom || models.length === 0);
  const valid = !explicit || model.trim() !== '';
  const dirty =
    provider !== '' && valid && (provider !== appliedProvider || model.trim() !== appliedModel);

  const providerOptions = [
    { value: AUTO, label: s.visionAuto },
    ...(appliedProvider !== AUTO && !vm.providers.some((entry) => entry.name === appliedProvider)
      ? [{ value: appliedProvider, label: appliedProvider }]
      : []),
    ...vm.providers.map((entry) => ({ value: entry.name, label: entry.name })),
  ];
  const modelOptions = [
    ...(model !== '' && !models.includes(model) ? [{ value: model, label: model }] : []),
    ...models.map((id) => ({ value: id, label: id })),
    { value: CUSTOM, label: s.customModel },
  ];

  const selectProvider = (next: string) => {
    setProvider(next);
    setCustom(false);
    if (next === AUTO) {
      setModel('');
      return;
    }
    const options = vm.providers.find((entry) => entry.name === next)?.models ?? [];
    if (options.length === 0) setModel('');
    else if (!options.includes(model)) setModel(options[0]);
  };

  const selectModel = (next: string) => {
    if (next === CUSTOM) {
      setCustom(true);
      setModel('');
      return;
    }
    setModel(next);
  };

  const apply = () =>
    cfg.set('speech.vision', {
      ...current,
      provider,
      model: provider === AUTO ? '' : model.trim(),
    });

  return (
    <div>
      <h3 className="text-sm font-semibold text-text-primary">{s.visionTitle}</h3>
      <p className="mt-1 text-xs text-text-tertiary">{s.visionHint}</p>
      <div className="mt-3 flex items-center gap-1.5">
        <SelectField
          label={s.providerLabel}
          value={provider}
          options={providerOptions}
          disabled={vm.loading || cfg.savingPath === 'speech.vision'}
          onChange={selectProvider}
        />
        {explicit &&
          (freeText ? (
            <TextInput
              label={s.modelLabel}
              value={model}
              placeholder={s.freeModelPlaceholder}
              disabled={cfg.savingPath === 'speech.vision'}
              onChange={setModel}
              onKeyDown={(event) => {
                if (event.key === 'Enter' && dirty) apply();
              }}
            />
          ) : (
            <SelectField
              label={s.modelLabel}
              value={model}
              options={modelOptions}
              disabled={cfg.savingPath === 'speech.vision'}
              onChange={selectModel}
            />
          ))}
        <Button size="sm" disabled={!dirty || cfg.savingPath === 'speech.vision'} onClick={apply}>
          {cfg.savingPath === 'speech.vision' ? s.applying : s.apply}
        </Button>
      </div>
    </div>
  );
}
