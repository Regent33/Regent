"use client";

import { BrailleVoiceViz } from "@/components/BrailleVoiceViz";
import { JarvisRing } from "@/components/JarvisRing";
import { useCall, type CallPhase } from "@/hooks/useCall";

const LABEL: Record<CallPhase, string> = {
  idle: "Tap to call Regent",
  connecting: "Connecting…",
  listening: "Listening — just talk",
  speaking: "Regent is speaking…",
  ended: "Call ended — tap to call again",
  error: "Something went wrong",
};

export function CallStage() {
  const { phase, error, analyser, start, stop } = useCall();
  const live = phase === "connecting" || phase === "listening" || phase === "speaking";
  const speaking = phase === "speaking";

  return (
    <main className="relative flex min-h-dvh flex-col items-center justify-center gap-9 px-6">
      {/* Jarvis core ring — sits behind the wordmark, reacts to call loudness */}
      <JarvisRing
        analyser={analyser}
        speaking={speaking}
        className="pointer-events-none absolute inset-0 mx-auto h-full w-full max-w-2xl"
      />

      <header className="absolute inset-x-0 top-0 flex items-center justify-center gap-2 p-5 text-xs tracking-[0.35em] text-regent-teal/70">
        <span className="text-regent-gold">♔</span>
        <span className="wordmark font-semibold">REGENT</span>
      </header>

      <div className="z-10 flex flex-col items-center gap-2">
        <span className="wordmark text-3xl font-semibold tracking-[0.32em]">REGENT</span>
        <span className="text-[0.65rem] uppercase tracking-[0.45em] text-regent-teal/60">
          live call
        </span>
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

      <button
        type="button"
        onClick={live ? stop : start}
        aria-label={live ? "End call" : "Start call"}
        className="z-10 flex h-20 w-20 items-center justify-center rounded-full border border-regent-teal/40 bg-regent-teal/10 text-regent-teal shadow-[0_0_50px_-6px_rgba(45,212,191,0.65)] transition hover:bg-regent-teal/20 focus:outline-none focus-visible:ring-2 focus-visible:ring-regent-teal active:scale-95"
      >
        {live ? <StopIcon /> : <MicIcon />}
      </button>
    </main>
  );
}

function MicIcon() {
  return (
    <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <rect x="9" y="3" width="6" height="11" rx="3" />
      <path d="M5 11a7 7 0 0 0 14 0" />
      <line x1="12" y1="18" x2="12" y2="21" />
    </svg>
  );
}

function StopIcon() {
  return (
    <svg width="24" height="24" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
      <rect x="6" y="6" width="12" height="12" rx="2.5" />
    </svg>
  );
}
