// Owner-only file permissions for secrets (.env). `mode: 0o600` is advisory
// on Windows, so there we additionally strip inherited ACEs and grant only
// the current user via icacls — the platform-native 0600. Best-effort: a
// failure never blocks the write (matches the unix chmod behavior).
import { spawnSync } from "node:child_process";

export function lockDownFile(path: string): void {
  if (process.platform !== "win32") return;
  const user = process.env.USERNAME;
  if (!user) return;
  spawnSync("icacls", [path, "/inheritance:r", "/grant:r", `${user}:F`], {
    stdio: "ignore",
  });
}
