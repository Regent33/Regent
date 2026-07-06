'use client';
// Voice section — voice.status summary (enabled + provider/model/available
// per ASR/TTS) and voice.models built-in picker lists. voice.set edits
// config/env and only applies on the next process start (surfaced verbatim).
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { ListRow } from '@/shared/ui/ListRow';
import { t } from '@/shared/i18n/t';
import { useVoiceSettings } from '@/features/settings/viewmodels/useVoiceSettings';

export function VoiceSection() {
  const s = t().settings.voice;
  const { status, models, loading, error, saving, note, setAsrModel, setTtsModel } = useVoiceSettings();

  return (
    <div className="p-6">
      <h2 className="text-lg font-semibold text-text-primary">{s.title}</h2>
      {loading && (
        <div className="mt-4">
          <Loader />
        </div>
      )}
      {error !== undefined && <ErrorState description={error} />}
      {!loading && error === undefined && status !== undefined && (
        <>
          <p className="mt-1 text-xs text-text-tertiary">{status.enabled ? s.enabled : s.disabled}</p>

          <h3 className="mt-5 text-sm font-semibold text-text-primary">{s.asrTitle}</h3>
          <p className="text-xs text-text-tertiary">
            {status.asrProvider ?? s.unset} · {status.asrModel ?? s.unset} ·{' '}
            {status.asrAvailable ? s.available : s.unavailable}
          </p>
          <div className="mt-2">
            {models.asrBuiltins.map((name) => (
              <ListRow
                key={name}
                label={name}
                active={status.asrModel === name}
                trailing={saving ? <Loader /> : undefined}
                onClick={() => setAsrModel(name)}
              />
            ))}
          </div>

          <h3 className="mt-5 text-sm font-semibold text-text-primary">{s.ttsTitle}</h3>
          <p className="text-xs text-text-tertiary">
            {status.ttsProvider ?? s.unset} · {status.ttsModel ?? s.unset} ·{' '}
            {status.ttsAvailable ? s.available : s.unavailable}
          </p>
          <div className="mt-2">
            {models.ttsBuiltins.map((name) => (
              <ListRow
                key={name}
                label={name}
                active={status.ttsModel === name}
                trailing={saving ? <Loader /> : undefined}
                onClick={() => setTtsModel(name)}
              />
            ))}
          </div>

          {note !== undefined && <p className="mt-3 text-xs text-text-tertiary">{note}</p>}
        </>
      )}
    </div>
  );
}
