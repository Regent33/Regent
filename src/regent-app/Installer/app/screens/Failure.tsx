import { Button } from "@/app/ui/Button";
import { PageHeader } from "@/app/ui/Logo";

export function Failure({
  error,
  log,
  onRetry,
  onBack,
}: {
  error: string | null;
  log: string[];
  onRetry: () => void;
  onBack: () => void;
}) {
  const copy = () => {
    const text = [error ?? "", ...log].join("\n");
    void navigator.clipboard?.writeText(text).catch(() => {});
  };

  return (
    <div className="mx-auto flex h-full max-w-2xl flex-col">
      <PageHeader
        title="Something went wrong"
        subtitle={error ?? "The install didn't finish."}
        tone="danger"
      />

      <div
        tabIndex={0}
        aria-label="Install log"
        className="mt-5 flex-1 select-text overflow-y-auto rounded-xl border border-stroke-tertiary bg-surface p-3 font-mono text-xs leading-relaxed text-text-tertiary"
      >
        {log.length === 0 ? (
          <span className="opacity-60">No output was captured.</span>
        ) : (
          log.map((l, i) => <div key={i}>{l}</div>)
        )}
      </div>

      <div className="mt-4 flex items-center justify-between">
        <Button variant="ghost" onClick={onBack}>
          Back
        </Button>
        <div className="flex gap-2">
          <Button variant="secondary" onClick={copy}>
            Copy log
          </Button>
          <Button onClick={onRetry}>Retry</Button>
        </div>
      </div>
    </div>
  );
}
