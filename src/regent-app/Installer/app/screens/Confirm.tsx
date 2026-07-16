import { Button } from "@/app/ui/Button";
import { PageHeader } from "@/app/ui/Logo";

// Uninstalling is destructive and one click from Apps & features, so it gets a
// confirmation — and the confirmation says what survives, not just what goes.
// "Will it delete my API keys?" is the only question worth answering here.
export function Confirm({
  installDir,
  onCancel,
  onUninstall,
}: {
  installDir: string;
  onCancel: () => void;
  onUninstall: () => void;
}) {
  return (
    <div className="mx-auto flex h-full max-w-2xl flex-col">
      <PageHeader title="Uninstall Regent" subtitle="We’re sorry to see you go." />

      {/* Future tense on both labels: nothing has happened yet, and a bare
          "Removed" on a confirmation reads as a claim that it already has. */}
      <div className="mt-6 space-y-4 text-sm">
        <div>
          <span className="mb-1.5 block text-xs font-medium uppercase tracking-wide text-text-tertiary">
            Will be removed
          </span>
          <p className="text-xs text-text-tertiary">
            The app, the agent core, and the{" "}
            <span className="font-mono">regent</span> CLI — plus the PATH entry,
            the desktop shortcut, and the Apps &amp; features listing.
          </p>
          <p className="mt-1 select-text break-all font-mono text-xs text-text-secondary">
            {installDir}
          </p>
        </div>

        <div className="rounded-xl border border-stroke-tertiary bg-surface p-3">
          <span className="mb-1.5 block text-xs font-medium uppercase tracking-wide text-accent">
            Will be kept
          </span>
          <p className="font-mono text-xs text-text-secondary">~/.regent</p>
          <p className="mt-1 text-xs text-text-tertiary">
            Your config, API keys, and memory stay where they are. Delete that
            folder yourself if you want them gone.
          </p>
        </div>
      </div>

      <div className="mt-auto flex items-center justify-between pt-6">
        <Button variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
        <Button variant="danger" onClick={onUninstall}>
          Uninstall
        </Button>
      </div>
    </div>
  );
}
