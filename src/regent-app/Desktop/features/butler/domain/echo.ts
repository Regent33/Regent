// Echo estimator for barge-in. Regent's TTS renders through a SEPARATE, capture-
// free AudioContext (see callLoop's PlaybackSink), so the mic's WebRTC echo
// cancellation never references it: on SPEAKERS his own voice bleeds back into
// the mic as speech-shaped energy the barge shape-vote can't reject, and it
// false-trips barge-in — Regent cuts himself off. This subtracts a *learned*
// estimate of that echo from the barge ENERGY only. On HEADPHONES there is no
// acoustic path, the coupling learns ~0, and it is a no-op (interruption stays
// exactly as sensitive as before). Pure + stateful (no DOM): the VAD loop reads
// the playback level and feeds it in, so the whole model is unit-testable.

const PLAY_ACTIVE = 0.01; // playback below this generates no learnable echo
const ECHO_LEARN = 0.2; // coupling EMA rate → ~92% converged in ~0.5s at ~43ms/frame
const LEARN_CEIL = 1.4; // warmup backstop: must exceed true coupling to bootstrap from 0 — loud speakers + an AGC'd mic reach ratios near 1, and 0.8 left them permanently unlearned (= self-barge forever)
const WARMUP_FRAMES = 8; // ~340ms of LEARNABLE frames after a reply starts: learn unconditionally (a barge is unlikely then)
const DT_FREEZE = 1.3; // post-warmup: full-rate learning stops once the mic exceeds predicted echo (double-talk onset)
const CREEP = 0.01; // ...but creep upward slowly instead of freezing: an AGC/volume ramp re-converges in seconds, while a real barge confirms in ~5 frames — far too few for this rate to ratchet coupling
const COUPLING_MAX = 1.5; // clamp; near feedback, fail safe toward an occasional self-interrupt, never toward suppressing the user
const SUPPRESS = 1.25; // margin on predicted echo for residual + echo-path latency-misalignment variance
const RELEASE = 0.85; // peak-hold decay/frame (~260ms release): spans echo latency + the sentence-gap ring-out, incl. buffered outputs (Bluetooth/WebView2) the old ~150ms undershot
const LAG_SETTLE = 3; // learn only after this many consecutive ACTIVE render frames: the acoustic echo lags the render, so onset frames pair a loud render with a still-quiet mic and would unlearn the coupling
const CORR_WINDOW = 16; // ~0.7s of envelope history for the correlation vote
const CORR_MAX_LAG = 7; // tolerate up to ~300ms of acoustic + output-buffer latency
const CORR_ECHO = 0.6; // a mic envelope tracking playback this tightly is echo, whatever the gain

export interface EchoEstimator {
  /** Call once when a reply first becomes audible (per turn) — starts the learn
   *  warmup and a fresh peak-hold. Does NOT forget the learned room coupling. */
  readonly startReply: () => void;
  /** Feed the frame's mic RMS and the current playback RMS; returns the echo-
   *  compensated mic level to use for the barge-in ENERGY decision. */
  readonly compensate: (micRms: number, playRms: number) => number;
  /** True while the mic envelope TRACKS the playback envelope (any gain, any
   *  lag up to ~300ms) — i.e. the mic is hearing Regent, not the caller. The
   *  gain-based subtraction above cannot follow a nonstationary mic (AGC /
   *  noise suppression reshapes gain mid-sentence); correlation is
   *  gain-invariant, so it stays true exactly when subtraction fails. */
  readonly echoLikely: () => boolean;
  /** Forget the learned room coupling — the device/room may have changed. */
  readonly reset: () => void;
}

/** Pearson correlation of `mic[i]` vs `play[offset + i]` over CORR_WINDOW. */
function laggedCorrelation(mic: readonly number[], play: readonly number[], offset: number): number {
  let micMean = 0;
  let playMean = 0;
  for (let i = 0; i < CORR_WINDOW; i++) {
    micMean += mic[i];
    playMean += play[offset + i];
  }
  micMean /= CORR_WINDOW;
  playMean /= CORR_WINDOW;
  let covariance = 0;
  let micVar = 0;
  let playVar = 0;
  for (let i = 0; i < CORR_WINDOW; i++) {
    const micDev = mic[i] - micMean;
    const playDev = play[offset + i] - playMean;
    covariance += micDev * playDev;
    micVar += micDev * micDev;
    playVar += playDev * playDev;
  }
  // A flat envelope on either side carries no evidence (also guards ÷0: a
  // silent playback window must never claim the mic as echo).
  if (micVar < 1e-10 || playVar < 1e-10 || playMean < PLAY_ACTIVE) return 0;
  return covariance / Math.sqrt(micVar * playVar);
}

export function createEchoEstimator(): EchoEstimator {
  let coupling = 0; // learned mic-echo / playback-render ratio (persists across turns)
  let playEnv = 0; // peak-hold envelope of the playback level (echo lags render + rings into gaps)
  let warmup = 0; // LEARNABLE frames since this reply became audible
  let active = 0; // consecutive frames of genuinely-rendering playback (not peak-hold coast)
  let micHist: number[] = []; // rolling mic envelope, newest last
  let playHist: number[] = []; // rolling render envelope, kept CORR_MAX_LAG longer

  return {
    startReply: () => {
      warmup = 0;
      playEnv = 0; // fresh per reply; no stale envelope from the last turn
      active = 0;
      micHist = [];
      playHist = [];
    },
    reset: () => {
      coupling = 0;
      playEnv = 0;
      warmup = 0;
      active = 0;
      micHist = [];
      playHist = [];
    },
    echoLikely: () => {
      if (micHist.length < CORR_WINDOW) return false;
      let best = 0;
      for (let lag = 0; lag <= CORR_MAX_LAG; lag++) {
        const offset = playHist.length - CORR_WINDOW - lag;
        if (offset < 0) break;
        best = Math.max(best, laggedCorrelation(micHist, playHist, offset));
      }
      return best > CORR_ECHO;
    },
    compensate: (micRms, playRms) => {
      // Peak-hold: the acoustic echo lags the rendered signal and keeps ringing
      // through the OS output buffer + room after playback goes idle between
      // sentences. A slow release keeps the estimate up across that lag instead
      // of snapping to 0 and false-barging at every sentence boundary.
      playEnv = Math.max(playRms, playEnv * RELEASE);
      const predicted = coupling * playEnv * SUPPRESS;
      const residual = Math.max(0, micRms - predicted);
      // Correlate what SUBTRACTION COULD NOT EXPLAIN, never the raw mic. On
      // speakers the raw mic carries Regent's echo continuously, so it tracks
      // the render envelope even while the caller is talking over him — the
      // veto then fired on every attempt and barge-in became impossible (the
      // reported "there's no barge in"). The residual answers the question
      // that actually matters: subtraction working leaves ~0 (and the energy
      // gate rejects it anyway); subtraction failing leaves echo that still
      // tracks the render, which is what this must veto; a caller talking
      // over leaves THEIR voice, which does not track it at all.
      micHist.push(residual);
      playHist.push(playRms);
      if (micHist.length > CORR_WINDOW) micHist.shift();
      if (playHist.length > CORR_WINDOW + CORR_MAX_LAG) playHist.shift();
      // Learn ONLY while the render is genuinely active AND settled. The old
      // gate (playEnv, the coasting peak-hold) kept "learning" through render
      // gaps and lagged onsets — frames that pair a high envelope with a
      // quiet mic — which collapsed the coupling toward 0 at full rate; the
      // echo then arrived, exceeded the tiny prediction, learning froze as
      // "double-talk", and Regent barged over himself at the next sentence.
      active = playRms > PLAY_ACTIVE ? active + 1 : 0;
      if (active > LAG_SETTLE) {
        const ratio = micRms / playEnv;
        // Warmup: learn unconditionally (capped by LEARN_CEIL) so a high-coupling
        // laptop speaker bootstraps from 0. After warmup: full-rate learning only
        // while the mic is explained by the clean estimate; unexplained energy
        // (a caller talking over) must not ratchet the coupling up — it creeps
        // instead, slow enough that only a persistent gain change (AGC, volume)
        // is ever absorbed.
        if (warmup < WARMUP_FRAMES ? ratio < LEARN_CEIL : micRms <= predicted * DT_FREEZE) {
          coupling = Math.min(COUPLING_MAX, coupling * (1 - ECHO_LEARN) + ratio * ECHO_LEARN);
        } else if (warmup >= WARMUP_FRAMES) {
          coupling = Math.min(COUPLING_MAX, coupling * (1 - CREEP) + Math.min(ratio, COUPLING_MAX) * CREEP);
        }
        warmup += 1;
      }
      return residual;
    },
  };
}
