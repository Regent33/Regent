import { Button } from "@/app/ui/Button";
import { LogoMark } from "@/app/ui/Logo";

// The uninstall counterpart of Finish. Same shape, same mark — the last thing
// you see leaving should look like the thing you installed.
export function Removed({ onClose }: { onClose: () => void }) {
  return (
    <div className="mx-auto flex h-full max-w-xl flex-col items-center justify-center pt-12 text-center">
      <LogoMark className="h-40 w-40" />
      <p className="-mt-3 text-xs font-medium uppercase tracking-[0.2em] text-accent">
        Removed
      </p>
      <h2 className="mt-2 font-display text-4xl text-text-primary">
        Regent is uninstalled
      </h2>
      <p className="mt-2 max-w-sm text-sm text-text-tertiary">
        Your <span className="font-mono text-text-secondary">~/.regent</span>{" "}
        folder was left alone — config, keys, and memory are still there if you
        come back.
      </p>
      <div className="mt-8">
        <Button className="px-6" onClick={onClose}>
          Close
        </Button>
      </div>
    </div>
  );
}
