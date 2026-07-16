import { useState } from "react";
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
  const [error, setError] = useState("");

  const set = <K extends keyof InstallOptions>(k: K, v: InstallOptions[K]) => {
    setError("");
    onChange({ ...options, [k]: v });
  };

  // Ask the backend whether this folder is usable before leaving the screen.
  // The field takes any path, but Setup is per-user with no elevation, so
  // `D:\Program Files\...` would otherwise fail several stages later with a raw
  // PowerShell error — long after the only screen that can fix it is gone.
  const install = async () => {
    setError("");
    let invoke: typeof import("@tauri-apps/api/core").invoke;
    try {
      ({ invoke } = await import("@tauri-apps/api/core"));
    } catch {
      onInstall(); // browser dev preview: no backend to ask, simulate on
      return;
    }
    try {
      await invoke("check_location", { dir: options.installDir });
    } catch (e) {
      setError(String(e));
      return;
    }
    onInstall();
  };

  // The picker returns the folder you chose — a home *for* Regent, not Regent's
  // own folder. Appending the name keeps a pick of `D:\Apps` from scattering
  // bin/ and app/ loose into it, and makes uninstall's remove-the-directory safe.
  // Already-named folders are left alone so browsing twice can't stutter into
  // `...\Regent\Regent`.
  const withRegent = (dir: string) => {
    const sep = dir.includes("\\") ? "\\" : "/";
    const trimmed = dir.replace(/[\\/]+$/, "");
    const last = trimmed.split(/[\\/]/).pop() ?? "";
    return last.toLowerCase() === "regent" ? trimmed : `${trimmed}${sep}Regent`;
  };

  // Native folder picker. In the browser dev preview there's no Tauri, so the
  // import throws and we just keep the typed path.
  const browse = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const picked = await open({
        directory: true,
        defaultPath: options.installDir,
      });
      if (typeof picked === "string") set("installDir", withRegent(picked));
    } catch {
      /* not running under Tauri */
    }
  };

  return (
    <div className="mx-auto flex h-full max-w-2xl flex-col">
      <PageHeader
        title="Install location"
        subtitle="Anywhere you like — Setup already has administrator rights."
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
            aria-invalid={error !== ""}
            aria-describedby={error ? "location-error" : undefined}
            className={error ? "border-danger" : ""}
          />
          <Button variant="secondary" onClick={browse}>
            Browse…
          </Button>
        </div>
        {error && (
          <p id="location-error" role="alert" className="mt-2 text-xs text-danger">
            {error}
          </p>
        )}
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
        <Button onClick={install} disabled={options.installDir.trim() === ""}>
          Install
        </Button>
      </div>
    </div>
  );
}
