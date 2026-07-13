'use client';
// The chosen camera for Butler Mode — mirror of mic.ts. Persisted to
// localStorage; Butler's getUserMedia video constraint reads it, the Voice
// settings section writes it. Labels need an active camera permission to be
// non-empty, so the picker enumerates after the first getUserMedia grant.

const KEY = 'regent.camera.deviceId';

export interface CameraDevice {
  readonly deviceId: string;
  readonly label: string;
}

/** The saved camera deviceId, or undefined = system default. */
export function getCameraDeviceId(): string | undefined {
  if (typeof localStorage === 'undefined') return undefined;
  const v = localStorage.getItem(KEY);
  return v === null || v === '' ? undefined : v;
}

/** Persist the pick; empty string clears it back to the system default. */
export function setCameraDeviceId(deviceId: string): void {
  if (typeof localStorage === 'undefined') return;
  if (deviceId === '') localStorage.removeItem(KEY);
  else localStorage.setItem(KEY, deviceId);
}

/** List video input devices. Labels are blank until a camera permission
 * exists, so callers should enumerate after a getUserMedia grant. */
export async function enumerateCameras(): Promise<readonly CameraDevice[]> {
  if (typeof navigator === 'undefined' || navigator.mediaDevices === undefined) return [];
  try {
    const devices = await navigator.mediaDevices.enumerateDevices();
    return devices
      .filter((d) => d.kind === 'videoinput')
      .map((d, i) => ({ deviceId: d.deviceId, label: d.label || `Camera ${i + 1}` }));
  } catch {
    return [];
  }
}

/** The video constraint for getUserMedia — pins the saved device when set.
 * Small frame: it only feeds the agent's camera_capture tool as a JPEG. */
export function cameraConstraint(): MediaTrackConstraints {
  const id = getCameraDeviceId();
  const base: MediaTrackConstraints = { width: { ideal: 640 } };
  return id === undefined ? base : { ...base, deviceId: { exact: id } };
}
