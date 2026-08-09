import { describe, expect, test } from 'bun:test';
import { API_KEY_GROUPS, toKey, visibleApiKeys } from './useApiKeys';

describe('API key grouping', () => {
  test('keeps messaging available to Gateway but out of the API Keys sections', () => {
    expect(toKey({ name: 'SLACK_BOT_TOKEN', group: 'messaging' })?.group).toBe('messaging');
    expect(API_KEY_GROUPS).not.toContain('messaging');
  });

  test('recognises local and the real media adapter groups', () => {
    expect(toKey({ name: 'OLLAMA_API_KEY', group: 'local' })?.group).toBe('local');
    expect(toKey({ name: 'REGENT_VISION_API_KEY', group: 'vision' })?.group).toBe('vision');
    expect(toKey({ name: 'REGENT_IMAGE_API_KEY', group: 'image' })?.group).toBe('image');
  });

  test('messaging-only data leaves the API Keys page visibly empty', () => {
    const messaging = toKey({ name: 'SLACK_BOT_TOKEN', group: 'messaging' });
    expect(visibleApiKeys(messaging === undefined ? [] : [messaging])).toEqual([]);
  });
});
