import { expect, test } from 'bun:test';
import { turnUpdate } from '@/shared/state/turnActivity';

const event = (method: string, params: Record<string, unknown> = {}) => ({ method, params });

test('turn and tool activity mark a session running', () => {
  for (const method of ['turn.started', 'tool.start', 'message.delta']) {
    expect(turnUpdate(event(method))).toEqual({ activity: 'running', error: null });
  }
});

test('turn completion stores an error or clears the previous one', () => {
  expect(turnUpdate(event('turn.complete', { error: 'provider failure: network error' }))).toEqual({
    activity: 'done',
    error: 'provider failure: network error',
  });
  expect(turnUpdate(event('turn.complete'))).toEqual({ activity: 'done', error: null });
  expect(turnUpdate(event('turn.interrupted'))).toEqual({ activity: 'done', error: null });
});

test('unrelated notifications do not change turn state', () => {
  expect(turnUpdate(event('model.changed', { model: 'x' }))).toBeUndefined();
});
