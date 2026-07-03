// The turn-based local call against the speech server: an energy-gated VAD
// loop that detects when the caller stops talking, POSTs the utterance to
// /call/turn, and plays the streamed reply (with barge-in).

import type { CallPhase } from "./useCall";
import { SPEECH_URL, fetchCallToken, type Playing, playPcm, wavBytes } from "./speechServer";

// VAD: RMS over this = speech; this many quiet frames (~85ms each) ends a turn.
const VAD_THRESHOLD = 0.015;
const VAD_HANG = 6;
// Barge-in: while Regent talks, this many sustained loud frames = you cut in
// (a bit above VAD so a cough/residual doesn't trigger it).
const INTERRUPT_THRESHOLD = 0.02;
const INTERRUPT_FRAMES = 3;
// Background-noise handling that CANNOT lock you out of talking: speech onset
// stays on the fixed VAD_THRESHOLD (exactly as before — so your voice always
// starts a turn), and the ambient RMS floor (rises slowly, falls fast) is used
// ONLY to raise the barge-in bar, so a fan/street/music can't cut Regent off.
const FLOOR_RISE = 0.01;
const FLOOR_FALL = 0.15;
const INTERRUPT_OVER_FLOOR = 3.5;
// A real word is ≥2 voiced frames (~170ms); shorter bursts (click, cough, key
// tap) are discarded with no round-trip. An utterance pinned open by constant
// noise force-ends at ~25s instead of hanging the call.
const MIN_VOICED_FRAMES = 2;
const MAX_UTTERANCE_FRAMES = 300;

// Camera → agent vision (mirrors the built-in /call page): while the call runs
// and the stream has a video track, POST a small JPEG every 2.5s to
// /call/frame, where the agent's camera_capture tool reads it while fresh.
const FRAME_INTERVAL_MS = 2500;

export function startCameraFrames(stream: MediaStream): () => void {
  if (!stream.getVideoTracks().length) return () => {};
  const video = document.createElement("video");
  video.muted = true;
  video.playsInline = true;
  video.srcObject = stream;
  void video.play().catch(() => {});
  const canvas = document.createElement("canvas");
  const timer = setInterval(async () => {
    if (!video.videoWidth) return;
    canvas.width = video.videoWidth;
    canvas.height = video.videoHeight;
    canvas.getContext("2d")?.drawImage(video, 0, 0);
    const token = await fetchCallToken();
    canvas.toBlob(
      (blob) => {
        if (!blob) return;
        void fetch(`${SPEECH_URL}/call/frame`, {
          method: "POST",
          body: blob,
          headers: { "x-call-token": token },
        }).catch(() => {});
      },
      "image/jpeg",
      0.7,
    );
  }, FRAME_INTERVAL_MS);
  return () => {
    clearInterval(timer);
    video.srcObject = null;
  };
}

export interface LocalCallSinks {
  setPhase: (p: CallPhase) => void;
  setHeard: (s: string) => void;
  setReply: (s: string) => void;
  setError: (s: string | null) => void;
}

/**
 * Energy-gated turn loop: detect when the caller stops talking, POST the
 * utterance to `/call/turn`, and play the streamed reply. Returns the processor
 * node so the caller can disconnect it on cleanup.
 */
export function startLocalCall(
  ctx: AudioContext,
  source: MediaStreamAudioSourceNode,
  node: AnalyserNode,
  sinks: LocalCallSinks,
): ScriptProcessorNode {
  // ponytail: ScriptProcessorNode is deprecated but works everywhere and needs no
  // separate worklet file (Next bundling). Swap for an AudioWorklet if it ever drops.
  const proc = ctx.createScriptProcessor(4096, 1, 1);
  source.connect(proc); // same source as the analyser — don't make a second one
  proc.connect(ctx.destination); // keep the node pulling; it outputs silence

  let buf: Float32Array[] = [];
  let speaking = false;
  let silence = 0;
  let busy = false; // a turn is in flight / Regent is talking
  let interruptFrames = 0;
  let turnGen = 0; // only the latest turn's completion may clear `busy`
  let abort: AbortController | null = null;
  const playing: Playing = { src: null };
  let noiseFloor = 0;
  let voiced = 0; // frames above the threshold this utterance
  let busyFrames = 0; // watchdog: frames spent busy (a hung turn must not wedge)
  let dbgPeak = 0;
  let dbgFrames = 0;
  console.debug("[call] VAD loop started (fixed-onset build)");

  const stopTurn = () => {
    abort?.abort(); // cancel the in-flight fetch/stream
    abort = null;
    if (playing.src) {
      try {
        playing.src.stop(); // fires onended → resolves the awaiting playPcm
      } catch {
        // already stopped
      }
      playing.src = null;
    }
  };

  proc.onaudioprocess = (e) => {
    const d = e.inputBuffer.getChannelData(0);
    let sum = 0;
    for (let i = 0; i < d.length; i++) sum += d[i] * d[i];
    const rms = Math.sqrt(sum / d.length);
    const a = rms > noiseFloor ? FLOOR_RISE : FLOOR_FALL;
    noiseFloor = noiseFloor * (1 - a) + rms * a;

    // Diagnostics: ~1/sec peak mic level + state, so a stuck call is legible.
    // If peakRMS stays ~0 while you talk, audio isn't reaching the loop; if it's
    // below thr, onset can't fire (mic too quiet). Remove once dialed in.
    dbgPeak = Math.max(dbgPeak, rms);
    if (++dbgFrames >= 12) {
      console.debug(
        `[call] peakRMS=${dbgPeak.toFixed(4)} thr=${VAD_THRESHOLD} speaking=${speaking} busy=${busy}`,
      );
      dbgPeak = 0;
      dbgFrames = 0;
    }

    if (busy) {
      // Liveness: while a reply is actively playing, the turn IS progressing —
      // long answers stream sentence-by-sentence, so speaking time must not
      // count toward the hung-turn watchdog (that was cutting replies off at ~20s).
      if (playing.src) busyFrames = 0;
      // Watchdog: a turn that sends NOTHING for ~20s (server hung, dropped
      // stream) must not wedge the call on busy — cancel and go back to listening.
      // `busyFrames` is reset on every streamed line (runTurn's onProgress) and
      // while audio plays, so this only trips on real silence, not a long reply.
      if (++busyFrames > 235) {
        stopTurn();
        turnGen += 1;
        busy = false;
        busyFrames = 0;
        speaking = false;
        interruptFrames = 0;
        buf = [];
        sinks.setPhase("listening");
        sinks.setError("That took too long — I reset. Try again.");
        return;
      }
      // Barge-in: you start talking while Regent is thinking/speaking. Echo
      // cancellation keeps Regent's voice out of the mic, so this is you —
      // gated well above the ambient floor so background noise never cuts in.
      if (rms > Math.max(INTERRUPT_THRESHOLD, noiseFloor * INTERRUPT_OVER_FLOOR)) {
        interruptFrames += 1;
        if (interruptFrames > INTERRUPT_FRAMES) {
          stopTurn();
          turnGen += 1; // invalidate the cancelled turn's `finally`
          busy = false;
          speaking = true; // start capturing this new utterance now
          silence = 0;
          interruptFrames = 0;
          voiced = 1;
          buf = [new Float32Array(d)];
          sinks.setPhase("listening");
        }
      } else {
        interruptFrames = 0;
      }
      return;
    }

    if (!speaking) {
      // Onset on the FIXED threshold — your voice always starts a turn, exactly
      // as before the noise work, so the call can never get stuck on "listening".
      if (rms > VAD_THRESHOLD) {
        speaking = true;
        voiced = 1;
        silence = 0;
        buf.push(new Float32Array(d));
      }
      return;
    }

    buf.push(new Float32Array(d));
    if (rms > VAD_THRESHOLD) {
      voiced += 1;
      silence = 0;
    } else {
      silence += 1;
    }
    if (silence > VAD_HANG || buf.length > MAX_UTTERANCE_FRAMES) {
      speaking = false;
      silence = 0;
      const utterance = buf;
      buf = [];
      if (voiced < MIN_VOICED_FRAMES) return; // noise blip — drop, stay listening
      busy = true;
      busyFrames = 0;
      interruptFrames = 0;
      abort = new AbortController();
      const myGen = ++turnGen;
      // onProgress: every line from the server proves the turn is alive, so the
      // hung-turn watchdog restarts its ~20s silence budget instead of firing mid-reply.
      void runTurn(utterance, ctx.sampleRate, ctx, node, sinks, abort.signal, playing, () => {
        if (myGen === turnGen) busyFrames = 0;
      }).finally(() => {
        if (myGen === turnGen) busy = false; // ignore a turn we barged over
      });
    }
  };

  return proc;
}

/** One turn: WAV-encode the utterance, stream /call/turn, play each audio chunk. */
async function runTurn(
  frames: Float32Array[],
  sampleRate: number,
  ctx: AudioContext,
  node: AnalyserNode,
  sinks: LocalCallSinks,
  signal: AbortSignal,
  playing: Playing,
  onProgress: () => void,
): Promise<void> {
  sinks.setPhase("thinking");
  let res: Response;
  try {
    res = await fetch(`${SPEECH_URL}/call/turn?language=English&speed=1`, {
      method: "POST",
      body: wavBytes(frames, sampleRate),
      headers: { "x-call-token": await fetchCallToken() },
      signal,
    });
  } catch (e) {
    if ((e as Error).name === "AbortError") return; // barged over
    sinks.setError("Speech server unreachable — run `regent voice serve` (port 8000).");
    sinks.setPhase("listening");
    return;
  }
  if (!res.body) {
    sinks.setPhase("listening");
    return;
  }
  sinks.setError(null);

  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let acc = "";
  let spokeYet = false;
  try {
    for (;;) {
      if (signal.aborted) return;
      const { done, value } = await reader.read();
      if (done) break;
      acc += decoder.decode(value, { stream: true });
      let nl: number;
      while ((nl = acc.indexOf("\n")) >= 0) {
        const line = acc.slice(0, nl);
        acc = acc.slice(nl + 1);
        if (!line.trim()) continue;
        let msg: { heard?: string; reply?: string; audio?: string; error?: string };
        try {
          msg = JSON.parse(line);
        } catch {
          continue;
        }
        onProgress(); // a valid line from the server → the turn is alive, reset the watchdog
        if (typeof msg.heard === "string") sinks.setHeard(msg.heard || "(didn't catch that)");
        if (typeof msg.reply === "string") sinks.setReply(msg.reply);
        if (msg.error) sinks.setError(msg.error);
        if (typeof msg.audio === "string") {
          if (signal.aborted) return;
          if (!spokeYet) {
            spokeYet = true;
            sinks.setPhase("speaking");
          }
          await playPcm(ctx, node, msg.audio, signal, playing); // sequential → gapless
        }
      }
    }
  } catch {
    return; // aborted reader read → swallow
  }
  if (!signal.aborted) sinks.setPhase("listening");
}
