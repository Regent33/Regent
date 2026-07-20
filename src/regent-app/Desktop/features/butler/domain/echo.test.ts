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
    // after the ~260ms release drains, the mic passes at full level again
    let level = 0;
    for (let i = 0; i < 26; i++) level = echo.compensate(0.1, 0);
    expect(level).toBeGreaterThan(0.095);
  });

  test('a render gap + lagged onset does not collapse the learned coupling', () => {
    // The reported self-barge root: quiet-mic frames during sentence gaps and
    // echo-lagged onsets used to unlearn the coupling at full rate; the echo
    // then exceeded the tiny prediction, learning froze as "double-talk", and
    // Regent cut himself off at the next sentence.
    const echo = createEchoEstimator();
    echo.startReply();
    learnEcho(echo, 0.5, 0.2);
    for (let i = 0; i < 6; i++) echo.compensate(0.001, 0); // inter-sentence gap: no render, room-quiet mic
    for (let i = 0; i < 3; i++) echo.compensate(0.001, 0.2); // next sentence renders; its echo hasn't arrived yet
    // the echo lands at full level — still fully subtracted, no self-barge
    expect(echo.compensate(0.1, 0.2)).toBe(0);
  });

  test('a persistent gain rise (AGC/volume) re-converges instead of barging forever', () => {
    const echo = createEchoEstimator();
    echo.startReply();
    learnEcho(echo, 0.5, 0.2);
    // the OS ramps mic gain: echo now lands at twice the learned level. The
    // upward creep must absorb it within a few seconds of speech...
    let level = 1;
    for (let i = 0; i < 200; i++) level = echo.compensate(0.2, 0.2);
    expect(level).toBeLessThan(0.01);
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
