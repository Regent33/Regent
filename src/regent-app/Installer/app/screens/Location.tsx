import { Button } from "@/app/ui/Button";
import { Checkbox, TextInput } from "@/app/ui/Field";
import type { InstallOptions } from "@/app/state";

export function Location({
  options,
  onChange,
  onBack,
  onInstall,
}: {
  options: InstallOptions;
  onChange: (o: InstallOptions) => void;
  onBack: () => void;
  onInstall: () => void;
}) {
  const set = <K extends keyof InstallOptions>(k: K, v: InstallOptions[K]) =>
    onChange({ ...options, [k]: v });

  // Phase 2 wires @tauri-apps/plugin-dialog's open({ directory: true }). In the
  // browser dev preview there's no native picker, so this is a no-op for now.
  const browse = () => {};

  return (
    <div className="mx-auto flex h-full max-w-2xl flex-col">
      <h2 className="font-display text-2xl text-text-primary">Install location</h2>
      <p className="mt-1 text-sm text-text-tertiary">
        Where Regent and its data will live.
      </p>

      <div className="mt-5">
        <span className="mb-1.5 block text-xs font-medium uppercase tracking-wide text-text-tertiary">
          Folder
        </span>
        <div className="flex gap-2">
          <TextInput
            value={options.installDir}
            onChange={(e) => set("installDir", e.target.value)}
            spellCheck={false}
            aria-label="Install folder"
          />
          <Button variant="secondary" onClick={browse}>
            Browse…
          </Button>
        </div>
      </div>

      <div className="mt-6 space-y-1">
        <Checkbox
          label="Add regent to PATH"
          hint="Run `regent` from any terminal."
          checked={options.addToPath}
          onChange={(v) => set("addToPath", v)}
        />
        <Checkbox
          label="Create a desktop shortcut"
          checked={options.desktopShortcut}
          onChange={(v) => set("desktopShortcut", v)}
        />
        <Checkbox
          label="Install for all users"
          hint="Requires administrator. Off = just you, no admin prompt."
          checked={options.allUsers}
          onChange={(v) => set("allUsers", v)}
        />
      </div>

      <div className="mt-auto flex items-center justify-between pt-6">
        <Button variant="ghost" onClick={onBack}>
          Back
        </Button>
        <Button onClick={onInstall} disabled={options.installDir.trim() === ""}>
          Install
        </Button>
      </div>
    </div>
  );
}
