// The barge-in echo estimator: it must SUBTRACT Regent's own speaker echo (so he
// can't cut himself off) without ever suppressing a real user who talks over him.
import { describe, expect, test } from 'bun:test';
import { createEchoEstimator } from './echo';

// Feed n echo-only frames (mic hears coupling*play) to teach the estimator.
function learnEcho(echo: ReturnType<typeof createEchoEstimator>, coupling: number, play: number, n = 30) {
  for (let i = 0; i < n; i++) echo.compensate(coupling * play, play);
}

describe('echo estimator', () => {
  test('headphones: no acoustic echo, so a real barge is never suppressed', () => {
    const echo = createEchoEstimator();
    echo.startReply();
    // mic hears only the room (~0.001) while the render level is high (0.3)
    learnEcho(echo, 0.0033, 0.3);
    // a real user barge passes through essentially untouched
    expect(echo.compensate(0.1, 0.3)).toBeGreaterThan(0.09);
  });

  test('speakers: a loud echo (high coupling) is learned and fully subtracted', () => {
    const echo = createEchoEstimator();
    echo.startReply();
    learnEcho(echo, 0.5, 0.2); // mic echo 0.1 = 0.5 * 0.2 render
    // that same echo level now nets to zero — it can't trip the barge gate
    expect(echo.compensate(0.1, 0.2)).toBe(0);
  });

  test('double-talk: a user over the echo survives, and cannot ratchet coupling up', () => {
    const echo = createEchoEstimator();
    echo.startReply();
    learnEcho(echo, 0.5, 0.2); // echo coupling ~0.5, predicted echo ~0.125
    // user talks OVER the echo: their voice above the subtracted echo survives
    expect(echo.compensate(0.25, 0.2)).toBeGreaterThan(0.05);
    // and learning froze at onset, so a subsequent echo-only frame is still killed
    // (no self-suppression creep — the noise-floor-ratchet failure class)
    expect(echo.compensate(0.1, 0.2)).toBe(0);
  });

  test('gap tail: peak-hold keeps subtracting the ring-out, then restores full sensitivity', () => {
    const echo = createEchoEstimator();
    echo.startReply();
    learnEcho(echo, 0.5, 0.2);
    // playback goes idle (playRms 0) but the room still rings: the echo-level
    // frame right after the clip is still suppressed, not a false barge.
    expect(echo.compensate(0.1, 0)).toBeLessThan(0.05);
    // after the ~150ms release drains, the mic passes at full level again
    let level = 0;
    for (let i = 0; i < 15; i++) level = echo.compensate(0.1, 0);
    expect(level).toBeGreaterThan(0.095);
  });

  test('reset forgets the room coupling (device/room may have changed)', () => {
    const echo = createEchoEstimator();
    echo.startReply();
    learnEcho(echo, 0.5, 0.2);
    echo.reset();
    echo.startReply();
    // nothing learned yet → the first frame is not suppressed
    expect(echo.compensate(0.1, 0.2)).toBeGreaterThan(0.05);
  });
});
