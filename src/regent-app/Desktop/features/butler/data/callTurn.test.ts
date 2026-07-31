import { afterEach, describe, expect, test } from 'bun:test';
import { runTextTurn } from './callTurn';
import type { CallSinks, PlaybackSink } from './callTypes';
import type { Playing } from './speechClient';

// NDJSON stream → Response, as the voice server would send it.
function ndjsonResponse(lines: object[]): Response {
  const body = lines.map((line) => `${JSON.stringify(line)}\n`).join('');
  return new Response(body, { headers: { 'content-type': 'application/x-ndjson' } });
}

const realFetch = globalThis.fetch;
afterEach(() => {
  globalThis.fetch = realFetch;
});

function mockServer(lines: object[]): void {
  globalThis.fetch = ((input: RequestInfo | URL) => {
    const url = String(input);
    if (url.includes('/call/token')) return Promise.resolve(Response.json({ token: 't' }));
    return Promise.resolve(ndjsonResponse(lines));
  }) as typeof fetch;
}

// playPcm bails out when decode fails — no AudioContext needed to trace order.
const playback = {
  ctx: { decodeAudioData: () => Promise.reject(new Error('no audio in tests')) },
  node: {},
} as unknown as PlaybackSink;

function recordingSinks(events: string[]): CallSinks {
  return {
    setPhase: (p) => events.push(`phase:${p}`),
    setHeard: () => {},
    setReply: () => events.push('reply'),
    setFiller: () => events.push('filler'),
    setError: () => {},
    waitForVisual: () => {
      events.push('gate');
      return Promise.resolve();
    },
    finalizeVisual: () => Promise.resolve(),
  };
}

async function run(lines: object[]): Promise<string[]> {
  mockServer(lines);
  const events: string[] = [];
  const playing: Playing = { src: null };
  await runTextTurn('hi', playback, recordingSinks(events), new AbortController().signal, playing, () => {});
  return events;
}

const AUDIO = 'AAAA'; // any base64 — decode is stubbed to fail fast

describe('Butler turn visual gate', () => {
  test('filler audio (before any reply) plays ungated; first real sentence waits', async () => {
    const events = await run([{ audio: AUDIO, i: 0 }, { reply: 'Hello.' }, { audio: AUDIO, i: 1 }]);
    // The filler speaks immediately — the gate is only consulted for the
    // first post-reply sentence, never ahead of it.
    expect(events.indexOf('phase:speaking')).toBeLessThan(events.indexOf('gate'));
    expect(events.filter((event) => event === 'gate')).toHaveLength(1);
  });

  test('without a filler, the first sentence still waits on the gate before speaking', async () => {
    const events = await run([{ reply: 'Hello.' }, { audio: AUDIO, i: 0 }]);
    expect(events.indexOf('gate')).toBeLessThan(events.indexOf('phase:speaking'));
    expect(events.filter((event) => event === 'gate')).toHaveLength(1);
  });

  // The filler's TEXT has to reach the client, not just its audio: the echo
  // veto matches what the mic heard against what Regent is saying, and while a
  // filler plays there is no reply text yet. Without this the veto was blind
  // for exactly that window and killed the turn.
  test('a filler line reaches the sink as text, and is not mistaken for a reply', async () => {
    const events = await run([
      { filler: 'Let me check.' },
      { audio: AUDIO, i: 0 },
      { reply: 'Hello.' },
      { audio: AUDIO, i: 1 },
    ]);
    expect(events).toContain('filler');
    // Still ungated: a filler must not count as the reply, or the first real
    // sentence would skip the visual gate.
    expect(events.indexOf('filler')).toBeLessThan(events.indexOf('reply'));
    expect(events.indexOf('phase:speaking')).toBeLessThan(events.indexOf('gate'));
  });
});

// The guard added on finalizeVisual must not cost the normal case: a turn that
// runs to completion still finalizes its visual. (The aborted branch is a race
// inside a single reader.read() and is not deterministically reachable from a
// unit test — it is closed by inspection, not pinned here.)
test('a turn that runs to completion still finalizes its visual', async () => {
  mockServer([{ heard: 'real question' }, { reply: 'Here you go.' }]);
  const events: string[] = [];
  const watched: CallSinks = {
    ...recordingSinks(events),
    finalizeVisual: () => {
      events.push('finalize');
      return Promise.resolve();
    },
  };
  await runTextTurn('hi', playback, watched, new AbortController().signal, { src: null }, () => {});
  expect(events).toContain('finalize');
});
