// Installer state — a plain state machine (6 screens); no state library needed
// for a linear wizard. App.tsx owns a single useReducer over this.

/** Which flow this binary is running — the Rust side routes on its own exe
 *  name and tells us at startup. See src-tauri/src/lib.rs. */
export type Mode = "install" | "uninstall";

export type Screen =
  | "welcome"
  | "license"
  | "location"
  | "confirm"
  | "progress"
  | "finish"
  | "removed"
  | "failure";

export interface InstallOptions {
  /** Where the Regent app + binaries land. Always per-user, so no UAC prompt
   *  and no elevated-relaunch path to get wrong. */
  installDir: string;
  addToPath: boolean;
  desktopShortcut: boolean;
}

export type StageStatus = "pending" | "running" | "done" | "failed";

export interface Stage {
  readonly id: string;
  readonly label: string;
  status: StageStatus;
}

/** Coarse stages — one per install-script invocation (Option A). The live log
 *  underneath carries the fine detail; per-substage spinners are a later add.
 *
 *  Uninstall reuses them: it touches the same three things, and the labels name
 *  *what* is affected, not the direction. */
export const STAGE_DEFS: readonly { id: string; label: string }[] = [
  { id: "core", label: "Agent core & CLI" },
  { id: "app", label: "Regent app" },
  { id: "wire", label: "PATH & shortcuts" },
];

/** Listed in the order the backend actually runs them, so the ticks land top to
 *  bottom. Uninstall removes the app before the core it depends on. */
export const freshStages = (mode: Mode = "install"): Stage[] => {
  const defs =
    mode === "uninstall"
      ? [STAGE_DEFS[1], STAGE_DEFS[0], STAGE_DEFS[2]]
      : STAGE_DEFS;
  return defs.map((s) => ({ ...s, status: "pending" }));
};

export const defaultOptions = (installDir: string): InstallOptions => ({
  installDir,
  addToPath: true,
  desktopShortcut: true,
});
