'use client';
// Microphone device picker for Butler Mode. Device labels/ids are hidden by
// the browser until a mic permission exists, so when none is granted yet we
// show a one-click "enable" that prompts, then re-enumerates. The choice
// persists to localStorage (shared/infrastructure/mic) — Butler reads it.
import { useCallback, useEffect, useState } from 'react';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { FieldRow, SelectField } from '@/features/settings/presentation/primitives';
import {
  type MicDevice,
  enumerateMics,
  getMicDeviceId,
  setMicDeviceId,
} from '@/shared/infrastructure/mic';

export function MicPicker() {
  const s = t().settings.voice;
  const [devices, setDevices] = useState<readonly MicDevice[]>([]);
  const [selected, setSelected] = useState(getMicDeviceId() ?? '');
  const [needsGrant, setNeedsGrant] = useState(false);

  const refresh = useCallback(async () => {
    const list = await enumerateMics();
    // Empty deviceIds ⇒ no permission yet (the browser withholds ids/labels).
    setNeedsGrant(list.length === 0 || list.every((d) => d.deviceId === ''));
    setDevices(list.filter((d) => d.deviceId !== ''));
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const grant = useCallback(async () => {
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      for (const track of stream.getTracks()) track.stop();
      await refresh();
    } catch {
      // Denied — leave the enable prompt; Butler's own flow guides the fix.
    }
  }, [refresh]);

  const pick = (id: string) => {
    setSelected(id);
    setMicDeviceId(id);
  };

  return (
    <FieldRow
      label={s.micLabel}
      description={s.micHint}
      control={
        needsGrant ? (
          <Button size="sm" onClick={grant}>
            {s.micGrant}
          </Button>
        ) : (
          <SelectField
            label={s.micLabel}
            value={selected}
            onChange={pick}
            options={[
              { value: '', label: s.micDefault },
              ...devices.map((d) => ({ value: d.deviceId, label: d.label })),
            ]}
          />
        )
      }
    />
  );
}
