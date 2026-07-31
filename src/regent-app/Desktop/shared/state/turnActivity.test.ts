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

// Stopping or barging in is something the person did on purpose. The backend
// still reports its reason ("core: interrupted"), and carrying it through
// painted a red error over the turn they had just chosen to cancel.
test('a deliberate interruption carries no error, even when one is reported', () => {
  expect(turnUpdate(event('turn.interrupted', { error: 'core: interrupted' }))).toEqual({
    activity: 'done',
    error: null,
  });
  // A real failure still surfaces.
  expect(turnUpdate(event('turn.complete', { error: 'core: interrupted' }))).toEqual({
    activity: 'done',
    error: 'core: interrupted',
  });
});

test('unrelated notifications do not change turn state', () => {
  expect(turnUpdate(event('model.changed', { model: 'x' }))).toBeUndefined();
});
