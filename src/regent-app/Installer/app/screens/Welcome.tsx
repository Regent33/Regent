import { Button } from "@/app/ui/Button";

export function Welcome({ onNext }: { onNext: () => void }) {
  return (
    <div className="mx-auto flex h-full max-w-xl flex-col items-center justify-center text-center">
      <img
        src="/regent-icon@2x.png"
        alt=""
        width={88}
        height={88}
        className="rounded-2xl shadow-[var(--shadow-elev)]"
        draggable={false}
      />
      <h1 className="mt-6 font-display text-5xl tracking-tight text-text-primary">
        REGENT
      </h1>
      <p className="mt-2 text-base text-text-secondary">Built to serve.</p>
      <p className="mt-6 max-w-md text-sm leading-relaxed text-text-tertiary">
        One step installs everything — the agent core, the{" "}
        <span className="font-mono text-text-secondary">regent</span> command-line
        tool, and the desktop app.
      </p>
      <Button className="mt-8 px-8 py-2.5 text-base" onClick={onNext}>
        Install
      </Button>
      <p className="mt-3 text-xs text-text-tertiary">
        Runs on your machine. Your keys and data never leave it.
      </p>
    </div>
  );
}
