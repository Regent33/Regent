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
 * Echo cancellation prevents Regent's voice from feeding back into barge-in;
 * browser noise suppression removes steady room/fan noise before VAD and ASR.
 * Auto gain stays off because it raises quiet background along with speech. */
export function micConstraint(): MediaTrackConstraints {
  const id = getMicDeviceId();
  const base: MediaTrackConstraints & { voiceIsolation?: boolean } = {
    echoCancellation: true,
    noiseSuppression: true,
    autoGainControl: false,
  };
  // Chromium/WebView2 exposes voiceIsolation on supported Windows audio
  // stacks. Unlike ordinary noise suppression, it targets competing voices
  // and media in the room. Keep it an ideal boolean so unsupported devices
  // continue with the standard WebRTC processing instead of failing capture.
  if (
    typeof navigator !== 'undefined' &&
    navigator.mediaDevices !== undefined &&
    'voiceIsolation' in navigator.mediaDevices.getSupportedConstraints()
  ) {
    base.voiceIsolation = true;
  }
  return id === undefined ? base : { ...base, deviceId: { exact: id } };
}
// ponytail: no Bluetooth-HFP mic steering. Switching capture off the headset
// to keep music in A2DP left the call capturing a mic the user wasn't speaking
// into → VAD never fired → "stuck on listening". The A2DP↔HFP switch is a
// driver-level consequence of opening a BT mic and can't be avoided at the app
// layer without breaking capture; use the user's real mic. Windows'
// comms-ducking of OTHER apps is handled separately (call_ducking_off).
