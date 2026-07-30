// The duck must never outlive the verification that raised it. A stuck duck
// leaves every later reply at 15% volume — audible as "Butler stopped
// answering" / stuck on Listening — and typed turns hid it by un-ducking.
import { describe, expect, test } from 'bun:test';
import { handleBusyFrame, type BusyDeps } from './bargeIn';
import { BUSY_WATCHDOG_FRAMES, createLoopState, type LoopState } from './loopState';

function deps(ducked: boolean[]): BusyDeps {
  return {
    playback: {
      node: { getFloatTimeDomainData: (a: Float32Array) => a.fill(0) },
      out: {},
      duck: (on: boolean) => ducked.push(on),
    } as unknown as BusyDeps['playback'],
    sinks: {
      setPhase: () => {},
      setHeard: () => {},
      setReply: () => {},
    setFiller: () => {},
      setError: () => {},
      waitForVisual: () => Promise.resolve(),
      finalizeVisual: () => Promise.resolve(),
    },
    stopTurn: () => {},
    verifyTurn: () => {},
  };
}

describe('the duck is always released', () => {
  test('the hung-turn watchdog un-ducks a reply it resets', () => {
    const ducked: boolean[] = [];
    const s: LoopState = createLoopState(32);
    s.busy = true;
    s.replyAudible = true;
    s.verify = { buf: [], silence: 0 }; // a barge was suspected and ducked
    const d = deps(ducked);
    // Silence until the watchdog trips.
    for (let i = 0; i <= BUSY_WATCHDOG_FRAMES + 1 && s.busy; i++) {
      handleBusyFrame(s, new Float32Array(512), 0, d);
    }
    expect(s.busy).toBe(false); // watchdog fired
    expect(ducked.at(-1)).toBe(false); // ...and restored the volume
  });
});
