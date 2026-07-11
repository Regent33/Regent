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

/** Windows names a Bluetooth headset's phone-call mic "… Hands-Free …". */
const HFP_RE = /hands.?free|\bhfp\b/i;

/** Butler's mic constraint, steering around Bluetooth Hands-Free inputs:
 * opening an HFP mic flips the whole headset from hi-fi (A2DP) to phone-call
 * mode, so everything ELSE the user is hearing (music in another window)
 * collapses to muffled call quality. When the DEFAULT mic is hands-free and
 * a wired/internal mic exists, capture from that instead — the headset stays
 * hi-fi. A device the user explicitly pinned in Voice settings always wins.
 * Labels need a prior mic grant — a first-ever call falls back unchanged. */
export async function butlerMicConstraint(): Promise<MediaTrackConstraints> {
  if (getMicDeviceId() !== undefined) return micConstraint();
  const mics = await enumerateMics();
  const dflt = mics.find((m) => m.deviceId === 'default');
  if (dflt === undefined || !HFP_RE.test(dflt.label)) return micConstraint();
  const wired = mics.find(
    (m) =>
      m.deviceId !== 'default' && m.deviceId !== 'communications' && m.label !== '' && !HFP_RE.test(m.label),
  );
  if (wired === undefined) return micConstraint();
  return { ...micConstraint(), deviceId: { exact: wired.deviceId } };
}
