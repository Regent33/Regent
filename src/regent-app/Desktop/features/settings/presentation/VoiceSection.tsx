'use client';
// Voice section — voice.status summary + per-kind provider picker and model
// field. The picker lists PROVIDERS (voice.models builtins) and writes
// voice.set {asr_provider|tts_provider}; the model is its own free-text
// field ({asr_model|tts_model}) — they are distinct config keys and the old
// code wrote providers into the model field (the "dead action" bug).
// Changes apply on the next voice-server start; the deacon's note renders
// verbatim.
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { ListRow } from '@/shared/ui/ListRow';
import { t } from '@/shared/i18n/t';
import { Section, FieldRow, TextField } from '@/features/settings/presentation/primitives';
import { MicPicker } from '@/features/settings/presentation/MicPicker';
import { useVoiceSettings } from '@/features/settings/viewmodels/useVoiceSettings';

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
          <FieldRow
            label={s.modelLabel}
            description={s.asrModelHint}
            control={
              <TextField
                label={s.modelLabel}
                value={status.asrModel ?? ''}
                applyLabel={s.apply}
                applying={vm.saving}
                onApply={vm.setAsrModel}
              />
            }
          />
          <FieldRow
            label={s.whisperSizeLabel}
            description={s.whisperSizeHint}
            control={
              <TextField
                label={s.whisperSizeLabel}
                value=""
                placeholder="tiny | base | small | medium"
                applyLabel={s.apply}
                applying={vm.saving}
                onApply={vm.setWhisperSize}
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
          <FieldRow
            label={s.modelLabel}
            description={s.ttsModelHint}
            control={
              <TextField
                label={s.modelLabel}
                value={status.ttsModel ?? ''}
                applyLabel={s.apply}
                applying={vm.saving}
                onApply={vm.setTtsModel}
              />
            }
          />

          {vm.note !== undefined && <p className="mt-3 text-xs text-text-tertiary">{vm.note}</p>}
        </>
      )}
    </Section>
  );
}
