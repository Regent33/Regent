"use client";

import { useEffect, useRef } from "react";
import { BrailleVoiceViz } from "@/components/BrailleVoiceViz";
import { JarvisRing } from "@/components/JarvisRing";
import { useCall, type CallPhase } from "@/hooks/useCall";

const LABEL: Record<CallPhase, string> = {
  idle: "Waking Regent…",
  connecting: "Connecting…",
  listening: "Listening — just talk",
  speaking: "Regent is speaking…",
  ended: "Call ended",
  error: "Something went wrong",
};

export function CallStage() {
  const { phase, error, analyser, start } = useCall();
  const started = useRef(false);
  const speaking = phase === "speaking";
  const live = phase === "connecting" || phase === "listening" || phase === "speaking";

  // Automatic: the call starts on load — no button. (Ref guards React's dev
  // double-mount so the mic is only requested once.)
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    void start();
  }, [start]);

  return (
    <main className="relative flex min-h-dvh flex-col items-center justify-center gap-8 px-6">
      {/* HUD gridlines behind everything (reference look) */}
      <div className="grid-bg pointer-events-none fixed inset-0 -z-10" />

      <header className="absolute inset-x-0 top-0 flex items-center justify-center gap-2 p-5 text-xs tracking-[0.35em] text-regent-teal/70">
        <span className="text-regent-gold">♔</span>
        <span className="wordmark font-semibold">REGENT</span>
      </header>

      {/* Small Jarvis ring with the wordmark inside it */}
      <div className="relative flex h-85 w-85 items-center justify-center">
        <JarvisRing analyser={analyser} speaking={speaking} className="absolute inset-0" />
        <div className="z-10 flex flex-col items-center gap-1">
          <span className="wordmark text-2xl font-semibold tracking-[0.3em]">REGENT</span>
          <span className="text-[0.6rem] uppercase tracking-[0.45em] text-regent-teal/60">
            live call
          </span>
        </div>
      </div>

      <BrailleVoiceViz analyser={analyser} speaking={speaking} className="z-10" />

      <p
        role="status"
        aria-live="polite"
        className={`z-10 min-h-5 text-sm ${error ? "text-amber-300/90" : "text-regent-teal/80"} ${
          live && !error ? "animate-breathe" : ""
        }`}
      >
        {error ?? LABEL[phase]}
      </p>
    </main>
  );
}
