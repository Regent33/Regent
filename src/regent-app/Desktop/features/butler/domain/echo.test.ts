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

  test('a mic that tracks the playback envelope reads as echo, at a lag and any gain', () => {
    const echo = createEchoEstimator();
    echo.startReply();
    // Speech-like modulated render; the mic hears it 3 frames late through an
    // AGC that has drifted to a completely different gain (2.1×). The
    // gain-based subtraction is blind to that — the correlation vote is not.
    const render = [0.3, 0.05, 0.25, 0.1, 0.32, 0.04, 0.28, 0.12, 0.3, 0.06, 0.27, 0.09, 0.31, 0.05, 0.26, 0.11, 0.29, 0.07, 0.3, 0.08, 0.28, 0.1, 0.32, 0.06];
    for (let i = 0; i < render.length; i++) {
      const lagged = i >= 3 ? render[i - 3] : 0.001;
      echo.compensate(2.1 * lagged, render[i]);
    }
    expect(echo.echoLikely()).toBe(true);
  });

  test('a steady caller voice over modulated playback is not echo', () => {
    const echo = createEchoEstimator();
    echo.startReply();
    const render = [0.3, 0.05, 0.25, 0.1, 0.32, 0.04, 0.28, 0.12, 0.3, 0.06, 0.27, 0.09, 0.31, 0.05, 0.26, 0.11, 0.29, 0.07, 0.3, 0.08];
    // the caller's sustained vowel dominates the mic: flat, uncorrelated
    for (const play of render) echo.compensate(0.2, play);
    expect(echo.echoLikely()).toBe(false);
  });

  test('a caller talking OVER the echo is not vetoed as echo', () => {
    // The live "there's no barge in" repro. On speakers the raw mic carries
    // Regent's echo continuously, so it tracks the render envelope even during
    // double-talk — correlating the RAW mic vetoed every interruption. What
    // subtraction cannot explain is the caller, and that does not track.
    const echo = createEchoEstimator();
    echo.startReply();
    const render = [0.3, 0.05, 0.25, 0.1, 0.32, 0.04, 0.28, 0.12, 0.3, 0.06, 0.27, 0.09, 0.31, 0.05, 0.26, 0.11, 0.29, 0.07, 0.3, 0.08, 0.28, 0.1, 0.32, 0.06];
    // learn the room first: mic hears only the echo at 0.5 coupling
    for (const play of render) echo.compensate(0.5 * play, play);
    // now the caller speaks over it — their own speech envelope, on top of the
    // same ongoing echo, moving independently of what Regent is rendering.
    const voice = [0.18, 0.22, 0.14, 0.26, 0.2, 0.24, 0.12, 0.28, 0.19, 0.23, 0.15, 0.27, 0.21, 0.25, 0.13, 0.29, 0.2, 0.22, 0.16, 0.26];
    for (let i = 0; i < voice.length; i++) {
      const play = render[i % render.length];
      echo.compensate(0.5 * play + voice[i], play);
    }
    expect(echo.echoLikely()).toBe(false);
  });

  test('with no playback rendering, the mic is never claimed as echo', () => {
    const echo = createEchoEstimator();
    echo.startReply();
    for (let i = 0; i < 24; i++) echo.compensate(0.1 + (i % 3) * 0.05, 0);
    expect(echo.echoLikely()).toBe(false);
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
