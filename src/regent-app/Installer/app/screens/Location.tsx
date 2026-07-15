import { Button } from "@/app/ui/Button";
import { Checkbox, TextInput } from "@/app/ui/Field";
import { PageHeader } from "@/app/ui/Logo";
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

  // Native folder picker. In the browser dev preview there's no Tauri, so the
  // import throws and we just keep the typed path.
  const browse = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const picked = await open({
        directory: true,
        defaultPath: options.installDir,
      });
      if (typeof picked === "string") set("installDir", picked);
    } catch {
      /* not running under Tauri */
    }
  };

  return (
    <div className="mx-auto flex h-full max-w-2xl flex-col">
      <PageHeader
        title="Install location"
        subtitle="Installed just for you — no administrator prompt."
      />

      <div className="mt-6">
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
