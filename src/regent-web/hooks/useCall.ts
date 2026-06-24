"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { Room, RoomEvent, Track, type RemoteTrack } from "livekit-client";

export type CallPhase = "idle" | "connecting" | "listening" | "speaking" | "ended" | "error";

export interface UseCall {
  phase: CallPhase;
  error: string | null;
  /** Analyser tracking whatever audio is currently loud — drives the visualizer. */
  analyser: AnalyserNode | null;
  start: () => Promise<void>;
  stop: () => void;
}

/**
 * One live call. Always opens the mic (so the braille viz reacts to your voice),
 * then — if `NEXT_PUBLIC_LIVEKIT_URL` is set — joins the LiveKit room where the
 * Regent agent answers. No server configured ⇒ local-mic preview (you still see
 * Jarvis react). The remote agent's audio is routed through the same analyser, so
 * the dots react to Regent talking too.
 */
export function useCall(): UseCall {
  const [phase, setPhase] = useState<CallPhase>("idle");
  const [error, setError] = useState<string | null>(null);
  const [analyser, setAnalyser] = useState<AnalyserNode | null>(null);

  const roomRef = useRef<Room | null>(null);
  const ctxRef = useRef<AudioContext | null>(null);
  const micRef = useRef<MediaStream | null>(null);
  const elsRef = useRef<HTMLAudioElement[]>([]);

  const cleanup = useCallback(() => {
    roomRef.current?.disconnect();
    roomRef.current = null;
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
    await ctx.resume();
    ctxRef.current = ctx;

    const node = ctx.createAnalyser();
    node.fftSize = 512;
    node.smoothingTimeConstant = 0.8;
    ctx.createMediaStreamSource(mic).connect(node);
    setAnalyser(node);
    setPhase("listening");

    const url = process.env.NEXT_PUBLIC_LIVEKIT_URL;
    if (!url) return; // local-mic preview — no LiveKit configured

    try {
      roomRef.current = await joinRoom(url, ctx, node, elsRef.current, setPhase);
    } catch (e) {
      // Live server unreachable — keep the local preview, but say why.
      setError(`Live server unavailable — local preview only. (${(e as Error).message})`);
    }
  }, []);

  const stop = useCallback(() => {
    cleanup();
    setPhase("ended");
  }, [cleanup]);

  return { phase, error, analyser, start, stop };
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
