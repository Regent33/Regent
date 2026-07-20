import { describe, expect, test } from 'bun:test';
import { type BusyDeps, handleBusyFrame } from './bargeIn';
import { createLoopState, type LoopState } from './loopState';

// A silent mic frame with a silent playback analyser — enough to drive the
// clip-playing / gap bookkeeping without any real audio.
const FRAME = new Float32Array(128);

function makeState(startReplyCalls: { count: number }): LoopState {
  const s = createLoopState(32);
  return {
    ...s,
    echo: {
      startReply: () => {
        startReplyCalls.count += 1;
      },
      compensate: () => 0,
      reset: () => {},
    },
  };
}

const deps: BusyDeps = {
  playback: {
    node: { getFloatTimeDomainData: (a: Float32Array) => a.fill(0) },
  } as unknown as BusyDeps['playback'],
  sinks: {
    setPhase: () => {},
    setHeard: () => {},
    setReply: () => {},
    setError: () => {},
    waitForVisual: () => Promise.resolve(),
    finalizeVisual: () => Promise.resolve(),
  },
  stopTurn: () => {},
};

function frames(s: LoopState, n: number, playing: boolean): void {
  s.playing.src = playing ? ({} as AudioBufferSourceNode) : null;
  for (let i = 0; i < n; i++) handleBusyFrame(s, FRAME, 0, deps);
}

describe('echo warmup re-arm across playback gaps', () => {
  test('a long gap (filler → think → answer) re-arms the warmup; a sentence gap does not', () => {
    const calls = { count: 0 };
    const s = makeState(calls);
    s.busy = true;

    frames(s, 5, true); // the filler plays
    expect(calls.count).toBe(1); // reply start arms the warmup once

    frames(s, 4, false); // ~170ms sentence-boundary gap
    frames(s, 5, true);
    expect(calls.count).toBe(1); // short gap: peak-hold covers it, no re-arm

    frames(s, 30, false); // >1s of thinking silence after the filler
    frames(s, 5, true); // the real answer begins
    expect(calls.count).toBe(2); // re-armed exactly once for the resumed onset
  });
});
