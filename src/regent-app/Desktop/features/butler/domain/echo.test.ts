// The barge-in echo estimator: it must SUBTRACT Regent's own speaker echo (so he
// can't cut himself off) without ever suppressing a real user who talks over him.
import { describe, expect, test } from 'bun:test';
import { createEchoEstimator } from './echo';
import { confirmsSpeechWindow, interruptGate } from './vad';

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

  test('reply warmup cannot self-trigger the five-frame barge vote', () => {
    const echo = createEchoEstimator();
    echo.startReply();
    let bargeVotes = 0;
    // Field reproduction: the matched analyser still exposed exactly five
    // opening echo frames above this gate — exactly the 5-of-8 confirmation.
    for (let frame = 0; frame < 5; frame += 1) {
      const level = echo.compensate(0.014, 0.13);
      if (level > 0.0078 && !echo.echoLikely()) bargeVotes += 1;
    }
    expect(bargeVotes).toBe(0);
    // The veto is only provisional; it releases when the existing warmup ends.
    for (let frame = 0; frame < 8; frame += 1) echo.compensate(0.014, 0.13);
    expect(echo.echoLikely()).toBe(false);
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
    // A MODULATED render, as real speech is. (A flat one carries no envelope
    // to correlate, so nothing could tell that echo from a caller's voice —
    // which is the very ambiguity the correlation vote exists to resolve.)
    const render = [0.3, 0.05, 0.25, 0.1, 0.32, 0.04, 0.28, 0.12, 0.3, 0.06, 0.27, 0.09, 0.31, 0.05, 0.26, 0.11, 0.29, 0.07, 0.3, 0.08, 0.28, 0.1, 0.32, 0.06];
    for (const play of render) echo.compensate(0.5 * play, play); // learn coupling ≈ 0.5
    // The OS ramps mic gain: the same echo now lands at twice the level and
    // still tracks the render, so the upward creep must absorb it.
    let level = 1;
    for (let round = 0; round < 20; round++) {
      for (const play of render) level = echo.compensate(1.0 * play, play);
    }
    expect(level).toBeLessThan(0.02);
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

  test('repeated talking-over cannot ratchet the coupling and kill barge-in', () => {
    // The "barge-in gets worse the longer it runs" repro. The post-warmup
    // upward creep used to fire on ANY unexplained energy, so every second a
    // caller talked over Regent inflated `coupling` toward a ratio that
    // included their own voice — permanently (it persists across turns) and
    // cumulatively, until predicted echo swamped any possible interruption.
    const echo = createEchoEstimator();
    echo.startReply();
    const render = [0.3, 0.05, 0.25, 0.1, 0.32, 0.04, 0.28, 0.12, 0.3, 0.06, 0.27, 0.09, 0.31, 0.05, 0.26, 0.11, 0.29, 0.07, 0.3, 0.08, 0.28, 0.1, 0.32, 0.06];
    for (const play of render) echo.compensate(0.5 * play, play); // learn coupling ≈ 0.5
    const bargeAt = (i: number) => echo.compensate(0.5 * render[i % render.length] + 0.2, render[i % render.length]);
    const first = bargeAt(0);
    // …several long, failed barge attempts across the reply…
    for (let attempt = 0; attempt < 12; attempt++) {
      for (let i = 0; i < 40; i++) bargeAt(i);
      for (const play of render) echo.compensate(0.5 * play, play); // echo-only between them
    }
    // …and the caller is still just as audible over the echo as on attempt one.
    expect(bargeAt(0)).toBeGreaterThan(first * 0.9);
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

// The field bug ("Regent goes muted mid-sentence, and hears his own voice as a
// barge"): the estimator was fed a 5.3ms playback SNAPSHOT (fftSize 256)
// against a 43ms mic frame. Speech is full of stop closures and word gaps tens
// of ms long, so the snapshot regularly read ~0 while the mic frame still
// carried the echo of the syllable before it. The model is only valid when the
// two measurements span the same time — this pins that contract on the code
// that would otherwise silently drift back.
describe('the playback level must span the mic frame, not a snapshot of it', () => {
  const MIC_ECHO = 0.014; // measured in the field (voice-server.log voiced_rms)
  const LOUD = 0.15;

  const BARGE_GATE = 0.0078; // interruptGate() in a quiet room

  /** Run `frames` of steady mic echo, pairing each with the render level a
   *  probe of `slots` × 5.3ms would have reported. Returns how many frames
   *  leaked past the barge gate. */
  function leaks(slots: number, frames = 400): number {
    // A speech-like render: loud syllables broken by ~10-20ms closures. A 43ms
    // probe spans a whole syllable+closure and never reads silent; a 5.3ms one
    // lands INSIDE closures, repeatedly and in runs.
    const env: number[] = [];
    for (let i = 0; env.length < frames * 8; i++) {
      for (let k = 0; k < 6 + (i % 5); k++) env.push(LOUD);
      for (let k = 0; k < 2 + (i % 3); k++) env.push(0.002);
    }
    const echo = createEchoEstimator();
    echo.startReply();
    let leaked = 0;
    for (let f = 0; f < frames; f++) {
      const win = env.slice((f + 1) * 8 - slots, (f + 1) * 8);
      const play = Math.sqrt(win.reduce((s, v) => s + v * v, 0) / win.length);
      if (echo.compensate(MIC_ECHO, play) > BARGE_GATE) leaked += 1;
    }
    return leaked;
  }

  test('a snapshot probe leaks several times more echo past the barge gate', () => {
    // The only variable is the probe width. Same room, same mic, same echo.
    const matched = leaks(8); // fftSize 2048 → ~43ms, spans the mic frame
    const snapshot = leaks(1); // fftSize 256 → ~5.3ms, the tap this replaced
    // Measured 5 vs 21 when this landed. Neither is zero — the estimator is a
    // model, not a null — but the snapshot leaks multiples more, and each leak
    // is a duck. Asserted as a ratio so tuning the estimator elsewhere doesn't
    // wedge this on an exact count.
    expect(snapshot).toBeGreaterThan(matched * 3);
  });

  test('and the correlation veto is blind in exactly that instant', () => {
    const echo = createEchoEstimator();
    echo.startReply();
    // A silent-looking render window carries no evidence, so echoLikely() cannot
    // rescue the frame the energy gate just let through — both fail together.
    for (let i = 0; i < 40; i++) echo.compensate(MIC_ECHO, 0.002);
    expect(echo.echoLikely()).toBe(false);
  });
});

// The whole point, asserted end to end: run the FULL barge decision — the
// energy vote AND the echo veto, combined exactly as data/bargeIn.ts combines
// them — against the mic level measured in the field while Regent speaks
// (voice-server.log, voiced_rms 0.0119-0.0159). It must never fire.
//
// Two fixes make this hold, and either one regressing brings the bug back: the
// estimator now sees a playback window as long as the mic frame (a 5.3ms
// snapshot read ~0 inside Regent's own stop closures), and the veto stays
// asserted through warmup (a cold estimator subtracts nothing, so the first
// five residuals could satisfy the five-frame vote by themselves).
describe('a Butler call cannot barge over itself', () => {
  const MIC = 0.014;
  const LOUD = 0.15;
  const GATE = interruptGate(0.002); // quiet room: the gate sits at its floor
  const ACTIVE = 5; // INTERRUPT_ACTIVE_FRAMES
  const WINDOW = 8; // INTERRUPT_WINDOW_FRAMES

  /** Regent speaking: loud syllables broken by 10-20ms stop closures. */
  function renderSlots(frames: number): number[] {
    const env: number[] = [];
    for (let i = 0; env.length < frames * 8; i++) {
      for (let k = 0; k < 6 + (i % 5); k++) env.push(LOUD);
      for (let k = 0; k < 2 + (i % 3); k++) env.push(0.002);
    }
    return env;
  }

  function selfBarges(warm: boolean, frames = 400): number {
    const env = renderSlots(frames);
    const echo = createEchoEstimator();
    // A warm estimator is any reply after the first: coupling persists across
    // turns, so both states have to be covered.
    if (warm) for (let i = 0; i < 60; i++) echo.compensate(MIC, LOUD);
    echo.startReply();
    const levels: number[] = [];
    const speechLike: boolean[] = [];
    let barges = 0;
    for (let f = 0; f < frames; f++) {
      const w = env.slice((f + 1) * 8 - 8, (f + 1) * 8);
      const play = Math.sqrt(w.reduce((s, v) => s + v * v, 0) / w.length);
      levels.push(echo.compensate(MIC, play));
      speechLike.push(true); // echo IS speech-shaped — the shape vote never rejects it
      if (levels.length > WINDOW) {
        levels.shift();
        speechLike.shift();
      }
      if (confirmsSpeechWindow(levels, speechLike, GATE, ACTIVE) && !echo.echoLikely()) {
        barges += 1;
        echo.startReply();
        levels.length = 0;
        speechLike.length = 0;
      }
    }
    return barges;
  }

  test('the first reply of a call, with nothing learned yet', () => {
    expect(selfBarges(false)).toBe(0);
  });

  test('every later reply, with the room coupling already learned', () => {
    expect(selfBarges(true)).toBe(0);
  });

  test('...and a real caller talking over him still gets through', () => {
    // The guard must not have been bought by making interruption impossible —
    // that is the failure the surrounding comments warn about repeatedly.
    const echo = createEchoEstimator();
    for (let i = 0; i < 60; i++) echo.compensate(MIC, LOUD); // learn the room
    echo.startReply();
    for (let i = 0; i < 10; i++) echo.compensate(MIC, LOUD); // finish warmup
    // A caller at their own speaking level, over the same playback.
    expect(echo.compensate(0.09, LOUD)).toBeGreaterThan(GATE);
  });
});
