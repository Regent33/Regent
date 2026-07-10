'use client';
// Voice section — voice.status summary + per-kind provider picker and model
// picker. The provider picker lists voice.models builtins and writes
// voice.set {asr_provider|tts_provider}; the model is its own config key
// ({asr_model|tts_model}). The deacon has no voice-model catalog op (unlike
// chat's providers.models), so the model dropdown draws from a curated
// frontend map per provider — uncertain ids are left out and an empty list
// (or the "Custom…" option) falls back to free text, matching
// MainModelPicker. Whisper size IS a closed set (the sherpa-onnx release
// bundles regent-voice-server downloads), so it's a plain SelectField.
// Changes apply on the next voice-server start; the deacon's note renders
// verbatim.
import { useState } from 'react';
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { ListRow } from '@/shared/ui/ListRow';
import { t } from '@/shared/i18n/t';
import { Section, FieldRow, TextField, SelectField } from '@/features/settings/presentation/primitives';
import { MicPicker } from '@/features/settings/presentation/MicPicker';
import { useVoiceSettings } from '@/features/settings/viewmodels/useVoiceSettings';

// The whisper release sizes regent-voice-server's sherpa-onnx download
// actually fetches (`sherpa-onnx-whisper-<size>.tar.bz2`) — the same set the
// Python fallback documents for REGENT_WHISPER_SIZE (python-voice-server/README.md).
// Kept as an explicit list rather than a live catalog: unlike chat models
// there's no `voice.models`-style enumeration for this axis, and it's a
// closed, rarely-changing set.
const WHISPER_SIZES = ['tiny', 'base', 'small', 'medium', 'large-v3'] as const;

// Curated model options per builtin speech provider (registry.rs names).
// Same bar as the deacon's chat provider_catalog: only ids verifiable from
// the providers' own docs; an uncertain provider is simply absent → free
// text. `local`/`piper`-style providers take machine-specific names, so
// they stay free text too.
const ASR_MODELS: Record<string, readonly string[]> = {
  groq: ['whisper-large-v3-turbo', 'whisper-large-v3'],
  openai: ['gpt-4o-transcribe', 'gpt-4o-mini-transcribe', 'whisper-1'],
  mistral: ['voxtral-mini-latest', 'voxtral-small-latest'],
  elevenlabs: ['scribe_v1'],
};
const TTS_MODELS: Record<string, readonly string[]> = {
  openai: ['gpt-4o-mini-tts', 'tts-1', 'tts-1-hd'],
  elevenlabs: ['eleven_multilingual_v2', 'eleven_turbo_v2_5', 'eleven_flash_v2_5'],
  minimax: ['speech-02-hd', 'speech-02-turbo'],
  gemini: ['gemini-2.5-flash-preview-tts', 'gemini-2.5-pro-preview-tts'],
  edge: ['en-US-AriaNeural', 'en-US-GuyNeural', 'en-GB-SoniaNeural'],
};

// Sentinel option value for "Custom…" — never a real model id.
const CUSTOM = '__custom__';

/** Model picker for one kind: a dropdown when the provider has curated
 * options (current value merged in, plus a Custom… free-text escape),
 * otherwise the free-text field. Writes voice.set on pick/apply. */
function VoiceModelField({
  options,
  value,
  hint,
  saving,
  onApply,
}: {
  options: readonly string[];
  value: string;
  hint: string;
  saving: boolean;
  onApply: (model: string) => void;
}) {
  const s = t().settings.voice;
  const customLabel = t().settings.model.customModel;
  const [custom, setCustom] = useState(false);
  const freeText = custom || options.length === 0;
  // The configured model may predate the curated list — keep it pickable.
  const merged = value !== '' && !options.includes(value) ? [value, ...options] : options;
  return (
    <FieldRow
      label={s.modelLabel}
      description={hint}
      control={
        freeText ? (
          <TextField label={s.modelLabel} value={value} applyLabel={s.apply} applying={saving} onApply={onApply} />
        ) : (
          <SelectField
            label={s.modelLabel}
            value={value}
            disabled={saving}
            options={[
              ...merged.map((m) => ({ value: m, label: m })),
              { value: CUSTOM, label: customLabel },
            ]}
            onChange={(m) => (m === CUSTOM ? setCustom(true) : onApply(m))}
          />
        )
      }
    />
  );
}

export function VoiceSection() {
  const s = t().settings.voice;
  const vm = useVoiceSettings();
  const { status, models } = vm;

  return (
    <Section title={s.title} description={status?.enabled === true ? s.enabled : s.disabled}>
      {vm.loading && <Loader />}
      {vm.error !== undefined && <ErrorState description={vm.error} />}
      {!vm.loading && vm.error === undefined && status !== undefined && (
        <>
          <h3 className="text-sm font-semibold text-text-primary">{s.micTitle}</h3>
          <MicPicker />

          <h3 className="mt-6 text-sm font-semibold text-text-primary">{s.asrTitle}</h3>
          <p className="text-xs text-text-tertiary">
            {status.asrProvider ?? s.unset} · {status.asrModel ?? s.unset} ·{' '}
            {status.asrAvailable ? s.available : s.unavailable}
          </p>
          <div className="mt-2">
            {models.asrBuiltins.map((name) => (
              <ListRow
                key={name}
                label={name}
                active={status.asrProvider === name}
                trailing={vm.saving ? <Loader /> : undefined}
                onClick={() => vm.setAsrProvider(name)}
              />
            ))}
          </div>
          <VoiceModelField
            key={status.asrProvider ?? 'asr'}
            options={ASR_MODELS[status.asrProvider ?? ''] ?? []}
            value={status.asrModel ?? ''}
            hint={s.asrModelHint}
            saving={vm.saving}
            onApply={vm.setAsrModel}
          />
          <FieldRow
            label={s.whisperSizeLabel}
            description={s.whisperSizeHint}
            control={
              <SelectField
                label={s.whisperSizeLabel}
                value={status.whisperSize ?? ''}
                disabled={vm.saving}
                options={WHISPER_SIZES.map((size) => ({ value: size, label: size }))}
                onChange={vm.setWhisperSize}
              />
            }
          />

          <h3 className="mt-6 text-sm font-semibold text-text-primary">{s.ttsTitle}</h3>
          <p className="text-xs text-text-tertiary">
            {status.ttsProvider ?? s.unset} · {status.ttsModel ?? s.unset} ·{' '}
            {status.ttsAvailable ? s.available : s.unavailable}
          </p>
          <div className="mt-2">
            {models.ttsBuiltins.map((name) => (
              <ListRow
                key={name}
                label={name}
                active={status.ttsProvider === name}
                trailing={vm.saving ? <Loader /> : undefined}
                onClick={() => vm.setTtsProvider(name)}
              />
            ))}
          </div>
          <VoiceModelField
            key={status.ttsProvider ?? 'tts'}
            options={TTS_MODELS[status.ttsProvider ?? ''] ?? []}
            value={status.ttsModel ?? ''}
            hint={s.ttsModelHint}
            saving={vm.saving}
            onApply={vm.setTtsModel}
          />

          {vm.note !== undefined && <p className="mt-3 text-xs text-text-tertiary">{vm.note}</p>}
        </>
      )}
    </Section>
  );
}
