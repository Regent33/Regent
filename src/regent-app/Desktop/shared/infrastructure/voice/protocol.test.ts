import { expect, test } from 'bun:test';
import { CALL_PROTOCOL, isCompatibleVoiceHealth, voiceLanguageHint } from './protocol';

test('a server without the current call protocol is stale, not healthy', () => {
  expect(isCompatibleVoiceHealth({ engine: 'old server' })).toBe(false);
  expect(isCompatibleVoiceHealth({ call_protocol: CALL_PROTOCOL - 1 })).toBe(false);
  expect(isCompatibleVoiceHealth({ call_protocol: CALL_PROTOCOL })).toBe(true);
});

test('browser locales become safe Whisper language hints', () => {
  expect(voiceLanguageHint('en-PH')).toBe('en');
  expect(voiceLanguageHint('JA_jp')).toBe('ja');
  expect(voiceLanguageHint('not-a-language')).toBeUndefined();
});
