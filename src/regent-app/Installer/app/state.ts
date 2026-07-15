// Installer state — a plain state machine (6 screens); no state library needed
// for a linear wizard. App.tsx owns a single useReducer over this.

export type Screen =
  | "welcome"
  | "license"
  | "location"
  | "progress"
  | "finish"
  | "failure";

export interface InstallOptions {
  /** Where the Regent app + binaries land. Defaults per-user (no UAC). */
  installDir: string;
  addToPath: boolean;
  allUsers: boolean;
  desktopShortcut: boolean;
}

export type StageStatus = "pending" | "running" | "done" | "failed";

export interface Stage {
  readonly id: string;
  readonly label: string;
  status: StageStatus;
}

/** Coarse stages — one per install-script invocation (Option A). The live log
 *  underneath carries the fine detail; per-substage spinners are a later add. */
export const STAGE_DEFS: readonly { id: string; label: string }[] = [
  { id: "core", label: "Agent core & CLI" },
  { id: "app", label: "Regent app" },
  { id: "wire", label: "PATH & shortcuts" },
];

export const freshStages = (): Stage[] =>
  STAGE_DEFS.map((s) => ({ ...s, status: "pending" }));

export const defaultOptions = (installDir: string): InstallOptions => ({
  installDir,
  addToPath: true,
  allUsers: false,
  desktopShortcut: true,
});
