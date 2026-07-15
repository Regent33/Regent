import { Button } from "@/app/ui/Button";
import { LogoMark } from "@/app/ui/Logo";
import type { InstallOptions } from "@/app/state";

export function Finish({ options }: { options: InstallOptions }) {
  // Starts the installed app and quits Setup. In the browser dev preview the
  // Tauri import throws, so this is a no-op there.
  const launch = async () => {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("launch_app", { installDir: options.installDir });
    } catch {
      /* not running under Tauri */
    }
  };

  return (
    <div className="mx-auto flex h-full max-w-xl flex-col items-center justify-center pt-12 text-center">
      <LogoMark className="h-40 w-40" />
      <p className="-mt-3 text-xs font-medium uppercase tracking-[0.2em] text-accent">
        Installed
      </p>
      <h2 className="mt-2 font-display text-4xl text-text-primary">
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
