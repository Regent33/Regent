import { describe, expect, test } from 'bun:test';
import { captionText } from './phase';

describe('captionText', () => {
  test('strips the markdown that leaked into the AC/DC caption', () => {
    const raw = '**Quick options for you:** 1. **YouTube** — search the track';
    expect(captionText(raw)).toBe('Quick options for you: 1. YouTube — search the track');
  });

  test('keeps "/" so AC/DC survives (unlike the speech sanitizer)', () => {
    expect(captionText('Playing *AC/DC* now')).toBe('Playing AC/DC now');
  });

  test('renders a markdown link as its text and drops inline code backticks', () => {
    expect(captionText('see [the video](https://y.example/x) or `run` it')).toBe(
      'see the video or run it',
    );
  });

  test('drops leading heading and bullet markers per line', () => {
    expect(captionText('## Title\n- one\n- two')).toBe('Title\none\ntwo');
  });

  test('plain text passes through unchanged', () => {
    expect(captionText('just a normal reply')).toBe('just a normal reply');
  });
});
