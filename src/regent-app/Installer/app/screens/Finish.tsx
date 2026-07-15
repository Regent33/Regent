import { Button } from "@/app/ui/Button";
import type { InstallOptions } from "@/app/state";

export function Finish({ options }: { options: InstallOptions }) {
  // Phase 2 wires invoke("launch_app") + app.exit(). No-op in the dev preview.
  const launch = () => {};

  return (
    <div className="mx-auto flex h-full max-w-xl flex-col items-center justify-center text-center">
      <div className="flex h-16 w-16 items-center justify-center rounded-2xl bg-accent text-3xl text-on-accent shadow-[var(--shadow-elev)]">
        ✓
      </div>
      <h2 className="mt-6 font-display text-3xl text-text-primary">
        You&apos;re all set
      </h2>
      <p className="mt-2 max-w-sm text-sm text-text-tertiary">
        Regent is installed. Open the app, or run{" "}
        <span className="font-mono text-text-secondary">regent</span> in a
        terminal to start in the CLI.
      </p>
      <div className="mt-8 flex gap-3">
        <Button className="px-6" onClick={launch}>
          Launch Regent
        </Button>
      </div>
      <p className="mt-4 select-text break-all text-xs text-text-tertiary">
        {options.installDir}
      </p>
    </div>
  );
}
