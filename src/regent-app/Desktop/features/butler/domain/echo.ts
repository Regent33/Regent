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

export interface EchoEstimator {
  /** Call once when a reply first becomes audible (per turn) — starts the learn
   *  warmup and a fresh peak-hold. Does NOT forget the learned room coupling. */
  readonly startReply: () => void;
  /** Feed the frame's mic RMS and the current playback RMS; returns the echo-
   *  compensated mic level to use for the barge-in ENERGY decision. */
  readonly compensate: (micRms: number, playRms: number) => number;
  /** Forget the learned room coupling — the device/room may have changed. */
  readonly reset: () => void;
}

export function createEchoEstimator(): EchoEstimator {
  let coupling = 0; // learned mic-echo / playback-render ratio (persists across turns)
  let playEnv = 0; // peak-hold envelope of the playback level (echo lags render + rings into gaps)
  let warmup = 0; // LEARNABLE frames since this reply became audible
  let active = 0; // consecutive frames of genuinely-rendering playback (not peak-hold coast)

  return {
    startReply: () => {
      warmup = 0;
      playEnv = 0; // fresh per reply; no stale envelope from the last turn
      active = 0;
    },
    reset: () => {
      coupling = 0;
      playEnv = 0;
      warmup = 0;
      active = 0;
    },
    compensate: (micRms, playRms) => {
      // Peak-hold: the acoustic echo lags the rendered signal and keeps ringing
      // through the OS output buffer + room after playback goes idle between
      // sentences. A slow release keeps the estimate up across that lag instead
      // of snapping to 0 and false-barging at every sentence boundary.
      playEnv = Math.max(playRms, playEnv * RELEASE);
      const predicted = coupling * playEnv * SUPPRESS;
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
      return Math.max(0, micRms - predicted);
    },
  };
}
