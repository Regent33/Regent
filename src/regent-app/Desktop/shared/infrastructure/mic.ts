'use client';
// The chosen microphone for Butler Mode. Persisted to localStorage so the
// pick survives restarts; Butler's getUserMedia reads it, the Voice settings
// section writes it. Labels need an active mic permission to be non-empty, so
// the picker enumerates after the first getUserMedia grant.

const KEY = 'regent.mic.deviceId';

export interface MicDevice {
  readonly deviceId: string;
  readonly label: string;
}

/** The saved input deviceId, or undefined = system default. */
export function getMicDeviceId(): string | undefined {
  if (typeof localStorage === 'undefined') return undefined;
  const v = localStorage.getItem(KEY);
  return v === null || v === '' ? undefined : v;
}

/** Persist the pick; empty string clears it back to the system default. */
export function setMicDeviceId(deviceId: string): void {
  if (typeof localStorage === 'undefined') return;
  if (deviceId === '') localStorage.removeItem(KEY);
  else localStorage.setItem(KEY, deviceId);
}

/** List audio input devices. Labels are blank until a mic permission exists,
 * so callers should enumerate after a getUserMedia grant. Empty off the web. */
export async function enumerateMics(): Promise<readonly MicDevice[]> {
  if (typeof navigator === 'undefined' || navigator.mediaDevices === undefined) return [];
  try {
    const devices = await navigator.mediaDevices.enumerateDevices();
    return devices
      .filter((d) => d.kind === 'audioinput')
      .map((d, i) => ({ deviceId: d.deviceId, label: d.label || `Microphone ${i + 1}` }));
  } catch {
    return [];
  }
}

/** The audio constraint for getUserMedia — pins the saved device when set.
 * echoCancellation stays ON (barge-in depends on Regent's own voice being
 * cancelled from the capture); noiseSuppression/autoGainControl are OFF —
 * they made captured speech sound processed/"noise cancelled" and add
 * nothing the voice server's own VAD/robustness doesn't already handle. */
export function micConstraint(): MediaTrackConstraints {
  const id = getMicDeviceId();
  const base: MediaTrackConstraints = {
    echoCancellation: true,
    noiseSuppression: false,
    autoGainControl: false,
  };
  return id === undefined ? base : { ...base, deviceId: { exact: id } };
}
// ponytail: no Bluetooth-HFP mic steering. Switching capture off the headset
// to keep music in A2DP left the call capturing a mic the user wasn't speaking
// into → VAD never fired → "stuck on listening". The A2DP↔HFP switch is a
// driver-level consequence of opening a BT mic and can't be avoided at the app
// layer without breaking capture; use the user's real mic. Windows'
// comms-ducking of OTHER apps is handled separately (call_ducking_off).
