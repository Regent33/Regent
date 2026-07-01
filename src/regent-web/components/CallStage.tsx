"use client";

import { useEffect, useRef } from "react";
import { BrailleVoiceViz } from "@/components/BrailleVoiceViz";
import { JarvisRing } from "@/components/JarvisRing";
import { useCall, type CallPhase } from "@/hooks/useCall";

const LABEL: Record<CallPhase, string> = {
  idle: "Waking Regent…",
  connecting: "Connecting…",
  listening: "Listening — just talk",
  thinking: "Thinking…",
  speaking: "Regent is speaking…",
  ended: "Call ended",
  error: "Something went wrong",
};

export function CallStage() {
  const { phase, error, heard, reply, analyser, start } = useCall();
  const started = useRef(false);
  const transcriptRef = useRef<HTMLDivElement>(null);
  const speaking = phase === "speaking";
  const live =
    phase === "connecting" ||
    phase === "listening" ||
    phase === "thinking" ||
    phase === "speaking";

  // Automatic: the call starts on load — no button. (Ref guards React's dev
  // double-mount so the mic is only requested once.)
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    void start();
  }, [start]);

  // Keep the transcript pinned to the latest line as the reply streams in, so a
  // long reply scrolls inside its panel instead of growing the page.
  useEffect(() => {
    const el = transcriptRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [reply]);

  return (
    <main className="relative flex min-h-dvh flex-col items-center justify-center gap-8 px-6">
      {/* HUD gridlines behind everything (reference look) */}
      <div className="grid-bg pointer-events-none fixed inset-0 -z-10" />

      <header className="absolute inset-x-0 top-0 flex items-center justify-center gap-2 p-5 text-xs tracking-[0.35em] text-regent-teal/70">
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

      {/* Live transcript (local call): what you said + Regent's reply. The reply
          lives in a bounded, auto-scrolling panel so a long answer stays readable
          (left-aligned) and never grows the page or shoves the ring off-screen. */}
      {(heard || reply) && (
        <div className="z-10 flex w-full max-w-2xl flex-col gap-2">
          {heard && <p className="text-center text-sm text-white/60">{heard}</p>}
          {reply && (
            <div
              ref={transcriptRef}
              className="max-h-[34vh] overflow-y-auto whitespace-pre-wrap rounded-xl border border-regent-teal/15 bg-black/40 px-4 py-3 text-left text-sm leading-relaxed text-regent-teal"
            >
              {plainText(reply)}
            </div>
          )}
        </div>
      )}
    </main>
  );
}

// Voice replies are markdown; the TTS speaks a symbol-stripped version, so show
// the transcript as clean prose too (headings/bold/links/bullets flattened).
// Cheap and streaming-safe — not a full parser; incomplete markers just pass
// through until the closing token arrives.
function plainText(md: string): string {
  return md
    .replace(/```[\s\S]*?```/g, "")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/!?\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/^#{1,6}\s+/gm, "")
    .replace(/^\s*---+\s*$/gm, "")
    .replace(/\*\*([^*]+)\*\*/g, "$1")
    .replace(/\*([^*]+)\*/g, "$1")
    .replace(/^\s*[-*]\s+/gm, "• ")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}
