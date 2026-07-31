import { expect, test } from 'bun:test';
import { t } from '@/shared/i18n/t';

// The bug behind this line: a background job finished, the work was done, and
// nothing appeared in the chat because job results only ride the user's NEXT
// message. The notice says the news exists; the detail still comes with the
// next turn, and the wording says so rather than implying the answer is here.
test('a finished job is announced and points at where the detail is', () => {
  const line = t().chat.transcript.jobFinished('serve the site', 'finished');
  expect(line).toContain('serve the site');
  expect(line).toContain('send a message');
});

// Reporting a timed-out or cancelled job as "finished" is the exact laundering
// wrap_prompt's note text forbids; the notice must not undo it.
test('a job that did not finish is never phrased as done', () => {
  for (const state of ['timed_out', 'cancelled', 'failed']) {
    const line = t().chat.transcript.jobFinished('serve the site', state);
    expect(line).not.toContain('finished');
    expect(line).toContain('serve the site');
  }
  expect(t().chat.transcript.jobFinished('x', 'timed_out')).toContain('timed out');
});
