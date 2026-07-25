import { expect, test } from 'bun:test';
import { classifyVoiceHealth } from '@/features/shell/viewmodels/useVoiceHealth';

const warm = { asr: true, tts: true, warm: true };

test('voice is ready only when engines and the Butler agent are ready', () => {
  expect(classifyVoiceHealth({ ...warm, agent: 'ready' })).toBe('ready');
  expect(classifyVoiceHealth({ ...warm, agent: 'warming up' })).toBe('warming');
});

test('a failed Butler agent prevents a false Voice ready state', () => {
  expect(classifyVoiceHealth({ ...warm, agent: "deacon didn't answer on stdio in 30s" })).toBe(
    'down',
  );
});

test('cold engines remain warming even if the agent is ready', () => {
  expect(classifyVoiceHealth({ asr: true, tts: false, warm: false, agent: 'ready' })).toBe(
    'warming',
  );
});

// ADR-041 §4.3: an absent additive field means an OLDER peer, so the caller
// degrades to the legacy contract (warm engines = ready) instead of pulsing
// amber forever against a voice server that predates the agent note.
test('an older voice server without the agent note degrades to the legacy contract', () => {
  expect(classifyVoiceHealth(warm)).toBe('ready');
});
