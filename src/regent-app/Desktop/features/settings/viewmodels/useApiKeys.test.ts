import { describe, expect, test } from 'bun:test';
import { API_KEY_GROUPS, toKey, visibleApiKeys } from './useApiKeys';

describe('API key grouping', () => {
  test('keeps messaging available to Gateway but out of the API Keys sections', () => {
    expect(toKey({ name: 'SLACK_BOT_TOKEN', group: 'messaging' })?.group).toBe('messaging');
    expect(API_KEY_GROUPS).not.toContain('messaging');
  });

  test('recognises local providers and dedicated media credential groups', () => {
    // LM Studio, not Ollama: local Ollama is keyless, so OLLAMA_API_KEY is the
    // HOSTED service's credential and the deacon now sends it as 'llm'. Using
    // it as the local example described a classification that no longer exists.
    expect(toKey({ name: 'LMSTUDIO_API_KEY', group: 'local' })?.group).toBe('local');
    expect(toKey({ name: 'OLLAMA_API_KEY', group: 'llm' })?.group).toBe('llm');
    expect(toKey({ name: 'REGENT_VISION_API_KEY', group: 'vision' })?.group).toBe('vision');
    expect(toKey({ name: 'REGENT_IMAGE_API_KEY', group: 'image' })?.group).toBe('image');
    expect(toKey({ name: 'RUNWAYML_API_SECRET', group: 'video' })?.group).toBe('video');
    expect(API_KEY_GROUPS).toContain('image');
    expect(API_KEY_GROUPS).toContain('video');
  });

  test('messaging-only data leaves the API Keys page visibly empty', () => {
    const messaging = toKey({ name: 'SLACK_BOT_TOKEN', group: 'messaging' });
    expect(visibleApiKeys(messaging === undefined ? [] : [messaging])).toEqual([]);
  });
});
