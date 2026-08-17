// Owner-only file permissions for secrets (.env). `mode: 0o600` is advisory
// on Windows, so there we additionally strip inherited ACEs and grant only
// the current user via icacls — the platform-native 0600. Secret writes fail
// closed: publishing a plaintext key with unknown ACLs is not a safe fallback.
import { spawnSync } from "node:child_process";

export interface CommandResult {
  readonly status: number | null;
  readonly stdout?: string;
  readonly stderr?: string;
}

export type CommandRunner = (command: string, args: readonly string[]) => CommandResult;

const runCommand: CommandRunner = (command, args) =>
  spawnSync(command, [...args], {
    encoding: "utf8",
    windowsHide: true,
  });

/** Windows half extracted so failure handling is testable on every platform. */
export function lockDownWindowsFile(path: string, run: CommandRunner = runCommand): void {
  const identity = run("whoami", []);
  const principal = identity.status === 0 ? identity.stdout?.trim() : undefined;
  if (!principal) {
    throw new Error("cannot protect the secret file: whoami did not return the process identity");
  }
  const acl = run("icacls", [path, "/inheritance:r", "/grant:r", `${principal}:F`]);
  if (acl.status !== 0) {
    const detail = acl.stderr?.trim();
    throw new Error(`cannot protect the secret file with icacls${detail ? `: ${detail}` : ""}`);
  }
}

export function lockDownFile(path: string): void {
  if (process.platform !== "win32") return;
  lockDownWindowsFile(path);
}
