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

function makeDeps(record?: { ducked?: boolean[]; verified?: Float32Array[][] }): BusyDeps {
  return {
    playback: {
      node: { getFloatTimeDomainData: (a: Float32Array) => a.fill(0) },
      out: {},
      duck: (on: boolean) => record?.ducked?.push(on),
    } as unknown as BusyDeps['playback'],
    sinks: {
      setPhase: () => {},
      setHeard: () => {},
      setReply: () => {},
    setFiller: () => {},
      setError: () => {},
      setQuestion: () => {},
      waitForVisual: () => Promise.resolve(),
      finalizeVisual: () => Promise.resolve(),
    },
    stopTurn: () => {},
    verifyTurn: (frames: Float32Array[]) => record?.verified?.push(frames),
  };
}

const deps: BusyDeps = makeDeps();

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

  test('ambient noise above the floor but far below the caller never even ducks', () => {
    const record = { ducked: [] as boolean[] };
    const s = speakingState();
    for (let i = 0; i < 12; i++) handleBusyFrame(s, toneFrame(), 0.03, makeDeps(record));
    expect(s.verify).toBeNull();
    expect(record.ducked).toHaveLength(0);
    expect(s.busy).toBe(true);
  });

  test('a caller-level voice ducks the reply and hands the audio to ASR verification', () => {
    const record = { ducked: [] as boolean[], verified: [] as Float32Array[][] };
    const s = speakingState();
    const d = makeDeps(record);
    for (let i = 0; i < 12; i++) handleBusyFrame(s, toneFrame(), 0.15, d);
    expect(record.ducked).toEqual([true]); // reply keeps playing, quietly
    expect(s.verify).not.toBeNull();
    expect(s.busy).toBe(true); // NOT cut — the server's ASR is the judge now
    // The suspect goes quiet → endpoint fires → audio posted for verification.
    for (let i = 0; i < 20 && s.verify; i++) handleBusyFrame(s, new Float32Array(512), 0, d);
    expect(record.verified).toHaveLength(1);
    expect(record.verified[0].length).toBeGreaterThan(0);
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
