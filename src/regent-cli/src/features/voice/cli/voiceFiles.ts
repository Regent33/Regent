// .env persistence for `regent voice` — an atomic write that preserves other
// keys. The gateway reads .env; config.yaml goes through the deacon's validated
// write (deaconConfig.setConfigKeys), not from here.
import { join } from "node:path";
import { updateDotenv } from "@shared/infrastructure/storage/dotenvFile.ts";

/** Upsert `KEY=value` lines in `$home/.env`, preserving the rest. Mode 0600. */
export function upsertEnv(home: string, updates: Record<string, string>): void {
  updateDotenv(join(home, ".env"), updates);
}
