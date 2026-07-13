'use client';
// Camera device picker for Butler Mode — mirror of MicPicker. Device labels
// are hidden until a camera permission exists, so with none granted we show a
// one-click "enable" that prompts, then re-enumerates. The choice persists to
// localStorage (shared/infrastructure/camera) — Butler reads it.
import { useCallback, useEffect, useState } from 'react';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { FieldRow, SelectField } from '@/features/settings/presentation/primitives';
import {
  type CameraDevice,
  enumerateCameras,
  getCameraDeviceId,
  setCameraDeviceId,
} from '@/shared/infrastructure/camera';

export function CameraPicker() {
  const s = t().settings.voice;
  const [devices, setDevices] = useState<readonly CameraDevice[]>([]);
  const [selected, setSelected] = useState(getCameraDeviceId() ?? '');
  const [needsGrant, setNeedsGrant] = useState(false);

  const refresh = useCallback(async () => {
    const list = await enumerateCameras();
    // Empty deviceIds ⇒ no permission yet (the browser withholds ids/labels).
    setNeedsGrant(list.length === 0 || list.every((d) => d.deviceId === ''));
    setDevices(list.filter((d) => d.deviceId !== ''));
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const grant = useCallback(async () => {
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ video: true });
      for (const track of stream.getTracks()) track.stop();
      await refresh();
    } catch {
      // Denied — leave the enable prompt; Butler works camera-less anyway.
    }
  }, [refresh]);

  const pick = (id: string) => {
    setSelected(id);
    setCameraDeviceId(id);
  };

  return (
    <FieldRow
      label={s.cameraLabel}
      description={s.cameraHint}
      control={
        needsGrant ? (
          <Button size="sm" onClick={grant}>
            {s.cameraGrant}
          </Button>
        ) : (
          <SelectField
            label={s.cameraLabel}
            value={selected}
            onChange={pick}
            options={[
              { value: '', label: s.cameraDefault },
              ...devices.map((d) => ({ value: d.deviceId, label: d.label })),
            ]}
          />
        )
      }
    />
  );
}
