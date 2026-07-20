// Shared contracts for the Butler call loop (see callLoop.ts).
import type { CallPhase } from '@/features/butler/domain/phase';

export interface CallSinks {
  setPhase: (p: CallPhase) => void;
  setHeard: (s: string) => void;
  setReply: (s: string) => void;
  setError: (s: string | null) => void;
  /** Holds the first voice chunk until a diagram is visibly ready. */
  waitForVisual: () => Promise<void>;
  /** Resolves an inline/artifact/fallback visual after the reply stream ends. */
  finalizeVisual: () => Promise<void>;
}

/** Where Regent's reply audio renders: a SEPARATE AudioContext from the mic's.
 * The mic context carries an echo-cancelled capture, which Windows opens as a
 * communications session — rendering through that same context put the reply
 * (and, via driver voice-call DSP, every other app's audio) onto the
 * phone-call processing path: muffled TTS, ducked music. A capture-free
 * context renders as plain media at full quality. */
export interface PlaybackSink {
  ctx: AudioContext;
  node: AnalyserNode;
}

export interface CallLoopController {
  readonly processor: ScriptProcessorNode;
  readonly submitText: (text: string) => void;
  readonly setMuted: (muted: boolean) => void;
  /** Monotonic timestamp of the newest mic callback (not AudioContext time). */
  readonly lastAudioFrameAt: () => number;
  readonly dispose: () => void;
}
