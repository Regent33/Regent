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
const LEARN_CEIL = 0.8; // warmup backstop: must exceed true coupling to bootstrap from 0
const WARMUP_FRAMES = 8; // ~340ms after a reply starts: learn unconditionally (a barge is unlikely then)
const DT_FREEZE = 1.3; // post-warmup: freeze learning once the mic exceeds predicted echo (double-talk onset)
const COUPLING_MAX = 1.5; // clamp; near feedback, fail safe toward an occasional self-interrupt, never toward suppressing the user
const SUPPRESS = 1.25; // margin on predicted echo for residual + echo-path latency-misalignment variance
const RELEASE = 0.75; // peak-hold decay/frame (~150ms release): spans echo latency + the sentence-gap ring-out

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
  let warmup = 0; // frames since this reply became audible

  return {
    startReply: () => {
      warmup = 0;
      playEnv = 0; // fresh per reply; no stale envelope from the last turn
    },
    reset: () => {
      coupling = 0;
      playEnv = 0;
      warmup = 0;
    },
    compensate: (micRms, playRms) => {
      // Peak-hold: the acoustic echo lags the rendered signal and keeps ringing
      // through the OS output buffer + room after playback goes idle between
      // sentences. A slow release keeps the estimate up across that lag instead
      // of snapping to 0 and false-barging at every sentence boundary.
      playEnv = Math.max(playRms, playEnv * RELEASE);
      const predicted = coupling * playEnv * SUPPRESS;
      if (playEnv > PLAY_ACTIVE) {
        const ratio = micRms / playEnv;
        // Warmup: learn unconditionally (capped by LEARN_CEIL) so a high-coupling
        // laptop speaker bootstraps from 0. After warmup: freeze the instant the
        // caller adds energy the clean estimate can't explain, so talking over
        // Regent can't ratchet the coupling up and suppress its own barge.
        const learn = warmup < WARMUP_FRAMES ? ratio < LEARN_CEIL : micRms <= predicted * DT_FREEZE;
        if (learn) {
          coupling = Math.min(COUPLING_MAX, coupling * (1 - ECHO_LEARN) + ratio * ECHO_LEARN);
        }
      }
      warmup += 1;
      return Math.max(0, micRms - predicted);
    },
  };
}
