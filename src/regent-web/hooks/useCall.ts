"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { Room, RoomEvent, Track, type RemoteTrack } from "livekit-client";

export type CallPhase =
  | "idle"
  | "connecting"
  | "listening"
  | "thinking"
  | "speaking"
  | "ended"
  | "error";

export interface UseCall {
  phase: CallPhase;
  error: string | null;
  /** Latest transcription of the caller, and Regent's reply (local mode). */
  heard: string;
  reply: string;
  /** Analyser tracking whatever audio is currently loud — drives the visualizer. */
  analyser: AnalyserNode | null;
  start: () => Promise<void>;
  stop: () => void;
}

// The local speech server (faster-whisper + Kokoro) — `regent voice serve`.
const SPEECH_URL = process.env.NEXT_PUBLIC_SPEECH_URL || "http://localhost:8000";

/**
 * One live call. Always opens the mic (so the viz reacts to your voice), then:
 *   • `NEXT_PUBLIC_LIVEKIT_URL` set → join the LiveKit room (cloud/duplex agent), or
 *   • otherwise → a turn-based **local** call against the Python speech server:
 *     VAD → POST /call/turn → play Kokoro's reply through the same analyser, so the
 *     Jarvis ring reacts to Regent talking too. Fully local, no LiveKit needed.
 */
export function useCall(): UseCall {
  const [phase, setPhase] = useState<CallPhase>("idle");
  const [error, setError] = useState<string | null>(null);
  const [heard, setHeard] = useState("");
  const [reply, setReply] = useState("");
  const [analyser, setAnalyser] = useState<AnalyserNode | null>(null);

  const roomRef = useRef<Room | null>(null);
  const ctxRef = useRef<AudioContext | null>(null);
  const micRef = useRef<MediaStream | null>(null);
  const procRef = useRef<ScriptProcessorNode | null>(null);
  const elsRef = useRef<HTMLAudioElement[]>([]);

  const cleanup = useCallback(() => {
    roomRef.current?.disconnect();
    roomRef.current = null;
    procRef.current?.disconnect();
    procRef.current = null;
    micRef.current?.getTracks().forEach((t) => t.stop());
    micRef.current = null;
    for (const el of elsRef.current) {
      el.pause();
      el.srcObject = null;
      el.remove();
    }
    elsRef.current = [];
    ctxRef.current?.close().catch(() => {});
    ctxRef.current = null;
    setAnalyser(null);
  }, []);

  useEffect(() => cleanup, [cleanup]); // stop the call on unmount

  const start = useCallback(async () => {
    setError(null);
    setPhase("connecting");

    let mic: MediaStream;
    try {
      mic = await navigator.mediaDevices.getUserMedia({
        audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true },
      });
    } catch {
      setError("Microphone blocked — allow it and tap again.");
      setPhase("error");
      return;
    }
    micRef.current = mic;

    const ctx = new AudioContext();
    ctxRef.current = ctx;
    void ctx.resume().catch(() => {});
    // Autoplay policy: if still suspended (no gesture yet), resume on first input.
    if (ctx.state === "suspended") {
      const resume = () => {
        void ctx.resume().catch(() => {});
        window.removeEventListener("pointerdown", resume);
        window.removeEventListener("keydown", resume);
      };
      window.addEventListener("pointerdown", resume);
      window.addEventListener("keydown", resume);
    }

    const node = ctx.createAnalyser();
    node.fftSize = 512;
    node.smoothingTimeConstant = 0.8;
    ctx.createMediaStreamSource(mic).connect(node);
    setAnalyser(node);
    setPhase("listening");

    const url = process.env.NEXT_PUBLIC_LIVEKIT_URL;
    if (url) {
      try {
        roomRef.current = await joinRoom(url, ctx, node, elsRef.current, setPhase);
        return; // joined the LiveKit agent (cloud/duplex)
      } catch {
        // LiveKit configured but unreachable → fall through to the local call,
        // so a call still works as long as `regent voice serve` is running.
      }
    }

    // Local turn-based call against the Python speech server (faster-whisper + Kokoro).
    setPhase("listening");
    procRef.current = startLocalCall(ctx, mic, node, {
      setPhase,
      setHeard,
      setReply,
      setError,
    });
  }, []);

  const stop = useCallback(() => {
    cleanup();
    setPhase("ended");
  }, [cleanup]);

  return { phase, error, heard, reply, analyser, start, stop };
}

interface LocalCallSinks {
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
function startLocalCall(
  ctx: AudioContext,
  mic: MediaStream,
  node: AnalyserNode,
  sinks: LocalCallSinks,
): ScriptProcessorNode {
  // ponytail: ScriptProcessorNode is deprecated but works everywhere and needs no
  // separate worklet file (Next bundling). Swap for an AudioWorklet if it ever drops.
  const proc = ctx.createScriptProcessor(4096, 1, 1);
  ctx.createMediaStreamSource(mic).connect(proc);
  proc.connect(ctx.destination); // keep the node pulling; it outputs silence

  let buf: Float32Array[] = [];
  let speaking = false;
  let silence = 0;
  let busy = false; // don't capture while a turn is in flight / Regent is talking

  proc.onaudioprocess = (e) => {
    if (busy) return;
    const d = e.inputBuffer.getChannelData(0);
    let sum = 0;
    for (let i = 0; i < d.length; i++) sum += d[i] * d[i];
    const rms = Math.sqrt(sum / d.length);
    if (rms > 0.015) {
      speaking = true;
      silence = 0;
      buf.push(new Float32Array(d));
    } else if (speaking) {
      silence += 1;
      buf.push(new Float32Array(d));
      if (silence > 6) {
        speaking = false;
        silence = 0;
        const utterance = buf;
        buf = [];
        busy = true;
        void runTurn(utterance, ctx.sampleRate, ctx, node, sinks).finally(() => {
          busy = false;
        });
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
): Promise<void> {
  sinks.setPhase("thinking");
  let res: Response;
  try {
    res = await fetch(`${SPEECH_URL}/call/turn?language=English&speed=1`, {
      method: "POST",
      body: wavBytes(frames, sampleRate),
    });
  } catch {
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
  for (;;) {
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
        if (!spokeYet) {
          spokeYet = true;
          sinks.setPhase("speaking");
        }
        await playPcm(ctx, node, msg.audio); // sequential → gapless playback
      }
    }
  }
  sinks.setPhase("listening");
}

/** Decode a base64 WAV chunk and play it through the analyser + speakers. */
async function playPcm(ctx: AudioContext, node: AnalyserNode, b64: string): Promise<void> {
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  let audioBuf: AudioBuffer;
  try {
    audioBuf = await ctx.decodeAudioData(bytes.buffer);
  } catch {
    return;
  }
  const src = ctx.createBufferSource();
  src.buffer = audioBuf;
  src.connect(node); // viz reacts to Regent
  src.connect(ctx.destination); // and you hear it
  await new Promise<void>((resolve) => {
    src.onended = () => resolve();
    src.start();
  });
}

/** Downsample float frames to 16 kHz mono PCM16 and wrap in a WAV container. */
function wavBytes(frames: Float32Array[], sampleRate: number): ArrayBuffer {
  const DST = 16000;
  let total = 0;
  for (const f of frames) total += f.length;
  const all = new Float32Array(total);
  let off = 0;
  for (const f of frames) {
    all.set(f, off);
    off += f.length;
  }
  const ratio = sampleRate / DST;
  const len = Math.floor(all.length / ratio);
  const pcm = new Int16Array(len);
  for (let i = 0; i < len; i++) {
    let v = all[Math.floor(i * ratio)] || 0;
    v = Math.max(-1, Math.min(1, v));
    pcm[i] = v < 0 ? v * 32768 : v * 32767;
  }
  const out = new ArrayBuffer(44 + len * 2);
  const dv = new DataView(out);
  const W = (o: number, s: string) => {
    for (let i = 0; i < s.length; i++) dv.setUint8(o + i, s.charCodeAt(i));
  };
  W(0, "RIFF");
  dv.setUint32(4, 36 + len * 2, true);
  W(8, "WAVE");
  W(12, "fmt ");
  dv.setUint32(16, 16, true);
  dv.setUint16(20, 1, true);
  dv.setUint16(22, 1, true);
  dv.setUint32(24, DST, true);
  dv.setUint32(28, DST * 2, true);
  dv.setUint16(32, 2, true);
  dv.setUint16(34, 16, true);
  W(36, "data");
  dv.setUint32(40, len * 2, true);
  for (let i = 0; i < len; i++) dv.setInt16(44 + i * 2, pcm[i], true);
  return out;
}

/** Join the LiveKit room, publish the mic, and tap the agent's audio into `analyser`. */
async function joinRoom(
  fallbackUrl: string,
  ctx: AudioContext,
  analyser: AnalyserNode,
  els: HTMLAudioElement[],
  setPhase: (p: CallPhase) => void,
): Promise<Room> {
  const room = new Room({ adaptiveStream: true, dynacast: true });

  room.on(RoomEvent.TrackSubscribed, (track: RemoteTrack) => {
    if (track.kind !== Track.Kind.Audio) return;
    const el = track.attach() as HTMLAudioElement; // actually play Regent's voice
    el.autoplay = true;
    el.style.display = "none";
    document.body.appendChild(el);
    els.push(el);
    // Tap the same audio into the analyser so the dots react to Regent talking.
    ctx.createMediaStreamSource(new MediaStream([track.mediaStreamTrack])).connect(analyser);
  });

  room.on(RoomEvent.ActiveSpeakersChanged, (speakers) => {
    const agentTalking = speakers.some((p) => p !== room.localParticipant);
    setPhase(agentTalking ? "speaking" : "listening");
  });
  room.on(RoomEvent.Disconnected, () => setPhase("ended"));

  const res = await fetch("/api/token?room=regent-call", { cache: "no-store" });
  if (!res.ok) throw new Error(`token ${res.status}`);
  const { token, url } = (await res.json()) as { token: string; url?: string };
  await room.connect(url || fallbackUrl, token);
  await room.localParticipant.setMicrophoneEnabled(true);
  return room;
}
