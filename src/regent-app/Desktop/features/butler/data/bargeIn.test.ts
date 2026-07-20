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
      echoLikely: () => false,
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

describe('barge-in vs. the caller-level gate', () => {
  // Speech-shaped frame (200 Hz tone) so only the ENERGY gate decides.
  function toneFrame(): Float32Array {
    const d = new Float32Array(512);
    for (let i = 0; i < d.length; i++) d[i] = 0.3 * Math.sin((2 * Math.PI * 200 * i) / 48000);
    return d;
  }

  function speakingState(): LoopState {
    const s = makeState({ count: 0 });
    // pass-through compensation + no echo veto: energy reaches the gate raw
    (s as { echo: LoopState['echo'] }).echo = {
      startReply: () => {},
      compensate: (micRms: number) => micRms,
      echoLikely: () => false,
      reset: () => {},
    };
    s.busy = true;
    s.userLevel = 0.2; // the caller speaks at ~0.2 RMS on this mic
    s.playing.src = {} as AudioBufferSourceNode;
    return s;
  }

  test('ambient noise above the floor but far below the caller cannot cut the reply', () => {
    const s = speakingState();
    for (let i = 0; i < 12; i++) handleBusyFrame(s, toneFrame(), 0.03, deps);
    expect(s.busy).toBe(true); // still speaking — birdsong-level input ignored
  });

  test('a voice at the caller-level ballpark still barges in', () => {
    const s = speakingState();
    for (let i = 0; i < 12; i++) handleBusyFrame(s, toneFrame(), 0.15, deps);
    expect(s.busy).toBe(false);
    expect(s.speaking).toBe(true); // the interruption seeds the next turn
  });
});

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
