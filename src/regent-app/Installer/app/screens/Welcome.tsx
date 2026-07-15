import { Button } from "@/app/ui/Button";

export function Welcome({ onNext }: { onNext: () => void }) {
  return (
    <div className="mx-auto flex h-full max-w-lg flex-col items-center justify-center text-center">
      {/* Fixed size (not vw) so it stays consistent across window sizes and its
          width lines up with the text block below. */}
      <h1 className="font-display text-[9.5rem] leading-[0.82] tracking-tight text-accent">
        REGENT
      </h1>
      <p className="mt-6 text-lg text-text-secondary">Built to serve.</p>
      <p className="mt-2 max-w-md text-sm text-text-tertiary">
        The agent core, the <span className="font-mono">regent</span> CLI, and
        the app — installed in one step.
      </p>
      <Button className="mt-10 px-9 py-2.5 text-base" onClick={onNext}>
        Install
      </Button>
    </div>
  );
}
