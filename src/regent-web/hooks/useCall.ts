"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { Room, RoomEvent, Track, type RemoteTrack } from "livekit-client";
import { startCameraFrames, startLocalCall } from "./localCall";

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

/**
 * One live call. Always opens the mic (so the viz reacts to your voice), then:
 *   • `NEXT_PUBLIC_LIVEKIT_URL` set → join the LiveKit room (cloud/duplex agent), or
 *   • otherwise → a turn-based **local** call against the speech server:
 *     VAD → POST /call/turn → play the reply through the same analyser, so the
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
  const stopFramesRef = useRef<(() => void) | null>(null);

  const cleanup = useCallback(() => {
    stopFramesRef.current?.();
    stopFramesRef.current = null;
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

    // Camera + mic in ONE prompt so the agent can see ("can you see me?",
    // "what am I holding?"); no camera / denied → audio-only call, as before.
    const audio = { channelCount: 1, echoCancellation: true, noiseSuppression: true };
    let mic: MediaStream;
    try {
      mic = await navigator.mediaDevices.getUserMedia({ audio, video: { width: { ideal: 640 } } });
    } catch {
      try {
        mic = await navigator.mediaDevices.getUserMedia({ audio });
      } catch {
        setError("Microphone blocked — allow it and tap again.");
        setPhase("error");
        return;
      }
    }
    micRef.current = mic;
    stopFramesRef.current = startCameraFrames(mic);

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

    // ONE source feeds both the analyser (viz) and the VAD — two separate
    // MediaStreamSource nodes off the same mic can leave the second one silent.
    const source = ctx.createMediaStreamSource(mic);
    const node = ctx.createAnalyser();
    node.fftSize = 512;
    node.smoothingTimeConstant = 0.8;
    source.connect(node);
    setAnalyser(node);
    setPhase("listening");

    // Default to the local call. LiveKit is opt-in (NEXT_PUBLIC_USE_LIVEKIT=1):
    // the .env.local ships a LiveKit URL, but most setups have no LiveKit server,
    // and trying to connect to a dead one just stalls before the local fallback.
    const useLivekit =
      process.env.NEXT_PUBLIC_USE_LIVEKIT === "1" ||
      process.env.NEXT_PUBLIC_USE_LIVEKIT === "true";
    const url = process.env.NEXT_PUBLIC_LIVEKIT_URL;
    if (useLivekit && url) {
      try {
        roomRef.current = await joinRoom(url, ctx, node, elsRef.current, setPhase);
        return; // joined the LiveKit agent (cloud/duplex)
      } catch {
        // configured but unreachable → fall through to the local call.
      }
    }

    // Local turn-based call against the speech server (`regent voice serve`).
    procRef.current = startLocalCall(ctx, source, node, {
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
