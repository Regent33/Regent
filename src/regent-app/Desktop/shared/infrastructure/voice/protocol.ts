/** Must match the Rust voice server's `CALL_PROTOCOL`. */
export const CALL_PROTOCOL = 4;

/** Whisper accepts ISO-639-1 language hints. Browser locales include a region
 * (`en-PH`) or underscore on some platforms, so keep only the safe base code. */
export function voiceLanguageHint(locale: string | null | undefined): string | undefined {
  const base = locale?.trim().split(/[-_]/, 1)[0]?.toLowerCase();
  return base && /^[a-z]{2}$/.test(base) ? base : undefined;
}

export function isCompatibleVoiceHealth(value: unknown): boolean {
  if (typeof value !== 'object' || value === null) return false;
  return (value as { call_protocol?: unknown }).call_protocol === CALL_PROTOCOL;
}
