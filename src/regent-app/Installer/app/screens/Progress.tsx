import { useEffect, useRef } from "react";
import { PageHeader } from "@/app/ui/Logo";
import type { Stage, StageStatus } from "@/app/state";

// The ✓/✕/dots carry status visually; this is the same information for anyone
// listening rather than looking.
const STATUS_TEXT: Record<StageStatus, string> = {
  pending: "waiting",
  running: "in progress",
  done: "done",
  failed: "failed",
};

export function Progress({ stages, log }: { stages: Stage[]; log: string[] }) {
  const logEnd = useRef<HTMLDivElement>(null);
  // Keep the newest log line in view as install output streams in.
  useEffect(() => {
    logEnd.current?.scrollIntoView({ block: "end" });
  }, [log]);

  return (
    <div className="mx-auto flex h-full max-w-2xl flex-col">
      <PageHeader
        title="Installing…"
        subtitle="This takes a minute. Keep this window open."
      />

      {/* The stages are the progress a screen reader needs; the log below streams
          far too fast to announce. */}
      <ul className="mt-6 space-y-0.5" role="status" aria-live="polite">
        {stages.map((s) => (
          <li key={s.id} className="flex items-center gap-3 px-2 py-1.5">
            <StageIcon status={s.status} />
            <span
              className={
                s.status === "pending"
                  ? "text-sm text-text-tertiary"
                  : "text-sm text-text-primary"
              }
            >
              {s.label}
            </span>
            <span className="sr-only">{STATUS_TEXT[s.status]}</span>
          </li>
        ))}
      </ul>

      <div
        tabIndex={0}
        aria-label="Install output"
        className="mt-5 flex-1 select-text overflow-y-auto rounded-xl border border-stroke-tertiary bg-surface p-3 font-mono text-xs leading-relaxed text-text-tertiary"
      >
        {log.length === 0 ? (
          <span className="opacity-60">Waiting…</span>
        ) : (
          log.map((l, i) => <div key={i}>{l}</div>)
        )}
        <div ref={logEnd} />
      </div>
    </div>
  );
}

const POP = "animate-[stage-pop_180ms_cubic-bezier(0.23,1,0.32,1)]";

function StageIcon({ status }: { status: StageStatus }) {
  if (status === "done")
    return (
      <span className={`inline-block w-4 text-center text-accent ${POP}`}>✓</span>
    );
  if (status === "failed")
    return (
      <span className={`inline-block w-4 text-center text-danger ${POP}`}>✕</span>
    );
  if (status === "running")
    return (
      <span className="flex w-4 items-center justify-center gap-0.5">
        {[0, 0.15, 0.3].map((d) => (
          <i
            key={d}
            className="loader-dot inline-block h-1.5 w-1.5 rounded-full bg-accent"
            style={{ animationDelay: `${d}s` }}
          />
        ))}
      </span>
    );
  return (
    <span className="flex w-4 justify-center">
      <span className="h-1.5 w-1.5 rounded-full bg-stroke-primary" />
    </span>
  );
}
