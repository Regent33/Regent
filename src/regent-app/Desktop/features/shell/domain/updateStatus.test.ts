import { describe, expect, test } from 'bun:test';
import { RELEASES_URL, parseUpdateStatus } from './updateStatus';

describe('parseUpdateStatus', () => {
  test('reads an available verdict and ignores extra fields', () => {
    const s = parseUpdateStatus({
      current: '0.1.0',
      latest: '0.2.0',
      available: true,
      checked_at: 123,
      source: 'network',
      note: 'ignored',
    });
    expect(s).not.toBeNull();
    expect(s?.latest).toBe('0.2.0');
    expect(s?.current).toBe('0.1.0');
  });

  test('no badge unless an upgrade is actually available', () => {
    expect(parseUpdateStatus({ current: '0.1.0', latest: '0.2.0', available: false })).toBeNull();
    expect(parseUpdateStatus({ current: '0.1.0', latest: null, available: true })).toBeNull();
    expect(parseUpdateStatus({ current: '0.1.0', latest: '', available: true })).toBeNull();
  });

  test('a null / non-object / mis-typed body renders nothing', () => {
    for (const bad of [null, undefined, 7, 'x', [], {}, { available: 'true' }]) {
      expect(parseUpdateStatus(bad)).toBeNull();
    }
  });

  test('the release URL is the fixed official page, not a remote value', () => {
    expect(RELEASES_URL).toBe('https://github.com/Regent33/Regent/releases/latest');
  });
});
