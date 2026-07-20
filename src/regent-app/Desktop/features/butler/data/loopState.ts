// Mutable per-call state for the VAD loop, shared by the frame handlers
// (bargeIn.ts, capture.ts) and the wiring in callLoop.ts. One object, passed
// by reference — the handlers mutate it exactly as the original closure did.
import { type Playing } from '@/features/butler/data/speechClient';
import { createEchoEstimator } from '@/features/butler/domain/echo';

export const PROCESSOR_SAMPLES = 2048; // ~43 ms at 48 kHz; halves endpoint granularity
export const ONSET_WINDOW_FRAMES = 4; // ~170 ms rolling window; no blocking startup calibration
export const ONSET_ACTIVE_FRAMES = 3; // one processed-audio dropout is tolerated
export const PRE_ROLL_FRAMES = 8; // retain ~340 ms so onset confirmation never clips speech
export const INTERRUPT_WINDOW_FRAMES = 8; // ~340 ms rolling barge-in window
export const INTERRUPT_ACTIVE_FRAMES = 5; // a deliberate interruption SUSTAINS (~215ms of 5/8 frames); a clatter/burst that cleared the old 3/5 doesn't
export const STATIONARY_TAIL_FRAMES = 6; // stable room tone must not prolong a turn
export const MIN_VOICED_FRAMES = 4; // ~170 ms: reject a burst, retain short commands
export const BUSY_WATCHDOG_FRAMES = 1050; // ~45s of true silence ends a hung turn
// A playback gap longer than this (~1s) is a real pause (filler → long think →
// answer), not a sentence boundary (~150ms peak-hold release covers those).
export const ECHO_REARM_QUIET_FRAMES = 24;
// Barge-in must also reach this fraction of the caller's OWN learned speech
// level: a real interruption is spoken AT the device; distant ambient beds
// (birdsong — the live repro) sit well below how the caller actually sounds.
export const USER_LEVEL_RATIO = 0.35;

export const average = (levels: readonly number[]) =>
  levels.reduce((sum, level) => sum + level, 0) / levels.length;

export const frameRms = (d: Float32Array) => {
  let sum = 0;
  for (let i = 0; i < d.length; i++) sum += d[i] * d[i];
  return Math.sqrt(sum / d.length);
};

export interface LoopState {
  buf: Float32Array[];
  speaking: boolean;
  silence: number;
  onsetLevels: number[];
  onsetSpeechLike: boolean[];
  preRoll: Float32Array[];
  busy: boolean;
  replyAudible: boolean; // has Regent's TTS begun this turn? gates barge-in vs. the thinking phase
  playQuiet: number; // busy frames since a clip last played; a long run re-arms the echo warmup
  interruptLevels: number[];
  interruptSpeechLike: boolean[];
  interruptBuf: Float32Array[];
  turnGen: number; // only the latest turn's completion may clear `busy`
  noiseFloor: number;
  userLevel: number; // EMA of the caller's own voiced-frame RMS — how loud THEY sound at this mic
  // A suspected barge being captured while the reply plays on, ducked. The
  // server's ASR (not an energy heuristic) will decide if it was real.
  verify: { buf: Float32Array[]; silence: number } | null;
  sustainLevels: number[];
  sustainSpeechLike: boolean[];
  voiced: number;
  busyFrames: number;
  micMuted: boolean;
  lastFrameAt: number;
  disposed: boolean;
  // Diagnostics (~1/sec): if peakRMS stays ~0 while you talk, audio isn't
  // reaching the loop (suspended context / wrong device); below thr, onset
  // can't fire (mic too quiet). Same instrument the source file shipped with.
  dbgPeak: number;
  dbgFrames: number;
  readonly playing: Playing;
  // Subtract Regent's own TTS echo from the barge energy so his voice can't
  // self-trip barge-in on speakers (his reply renders on a separate context the
  // mic's AEC can't cancel). Persists across turns; reset only in recalibrateRoom.
  readonly echo: ReturnType<typeof createEchoEstimator>;
  readonly echoScratch: Float32Array<ArrayBuffer>;
}

export function createLoopState(playbackFftSize: number): LoopState {
  return {
    buf: [],
    speaking: false,
    silence: 0,
    onsetLevels: [],
    onsetSpeechLike: [],
    preRoll: [],
    busy: false,
    replyAudible: false,
    playQuiet: 0,
    interruptLevels: [],
    interruptSpeechLike: [],
    interruptBuf: [],
    turnGen: 0,
    noiseFloor: 0,
    userLevel: 0,
    verify: null,
    sustainLevels: [],
    sustainSpeechLike: [],
    voiced: 0,
    busyFrames: 0,
    micMuted: false,
    lastFrameAt: performance.now(),
    disposed: false,
    dbgPeak: 0,
    dbgFrames: 0,
    playing: { src: null },
    echo: createEchoEstimator(),
    echoScratch: new Float32Array(playbackFftSize),
  };
}

export const resetInterrupt = (s: LoopState) => {
  s.interruptLevels = [];
  s.interruptSpeechLike = [];
  s.interruptBuf = [];
  s.verify = null;
};

export const resetCapture = (s: LoopState) => {
  s.buf = [];
  s.speaking = false;
  s.silence = 0;
  s.onsetLevels = [];
  s.onsetSpeechLike = [];
  s.preRoll = [];
  resetInterrupt(s);
  s.sustainLevels = [];
  s.sustainSpeechLike = [];
  s.voiced = 0;
};

export const recalibrateRoom = (s: LoopState) => {
  resetCapture(s);
  s.noiseFloor = 0;
  s.userLevel = 0; // device/room changed — relearn how loud the caller sounds
  s.echo.reset();
};
