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

    if (busy) {
      // Barge-in: you start talking while Regent is thinking/speaking. Echo
      // cancellation keeps Regent's voice out of the mic, so this is you.
      if (rms > INTERRUPT_THRESHOLD) {
        interruptFrames += 1;
        if (interruptFrames > INTERRUPT_FRAMES) {
          stopTurn();
          turnGen += 1; // invalidate the cancelled turn's `finally`
          busy = false;
          speaking = true; // start capturing this new utterance now
          silence = 0;
          interruptFrames = 0;
          buf = [new Float32Array(d)];
          sinks.setPhase("listening");
        }
      } else {
        interruptFrames = 0;
      }
      return;
    }

    if (rms > VAD_THRESHOLD) {
      speaking = true;
      silence = 0;
      buf.push(new Float32Array(d));
    } else if (speaking) {
      silence += 1;
      buf.push(new Float32Array(d));
      if (silence > VAD_HANG) {
        speaking = false;
        silence = 0;
        const utterance = buf;
        buf = [];
        busy = true;
        interruptFrames = 0;
        abort = new AbortController();
        const myGen = ++turnGen;
        void runTurn(utterance, ctx.sampleRate, ctx, node, sinks, abort.signal, playing).finally(
          () => {
            if (myGen === turnGen) busy = false; // ignore a turn we barged over
          },
        );
      }
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
