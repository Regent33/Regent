// First-run detection for the onboarding wizard. The old gate — "does
// config.yaml exist?" — was defeatable: any command that boots the deacon
// (e.g. `regent model list`) seeds a full config.yaml, silently skipping the
// wizard forever. The wizard is needed until either it has completed once
// (marker) or credentials exist (.env — pre-marker installs stay quiet).
import { existsSync, writeFileSync } from "node:fs";
import { join } from "node:path";

export const SETUP_MARKER = ".setup-done";

export function needsOnboarding(home: string): boolean {
  return !existsSync(join(home, SETUP_MARKER)) && !existsSync(join(home, ".env"));
}

// Wizard completed (with or without a key) — never auto-show it again.
// The marker lives beside the user's data, so `uninstall --purge` resets it.
export function markSetupDone(home: string): void {
  writeFileSync(join(home, SETUP_MARKER), `${new Date().toISOString()}\n`);
}
