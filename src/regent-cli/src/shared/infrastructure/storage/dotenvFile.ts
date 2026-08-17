// One secure TypeScript persistence path for Regent's shared .env.
// Every caller gets the same cross-process lock, duplicate normalization,
// owner-only temporary, fsync, and atomic publish behavior.
import { randomUUID } from "node:crypto";
import {
  closeSync,
  fsyncSync,
  openSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { basename, dirname, join } from "node:path";
import { withDotenvLock } from "./dotenvLock.ts";
import { lockDownFile } from "./lockdown.ts";

export type DotenvUpdates = Readonly<Record<string, string | null>>;
export type ProtectSecretFile = (path: string) => void;

export function readDotenvLines(path: string): string[] {
  try {
    return readFileSync(path, "utf8")
      .replace(/^\uFEFF/, "")
      .split(/\r?\n/);
  } catch {
    return [];
  }
}

function envName(line: string): string {
  const trimmed = line.trimStart();
  const equals = trimmed.indexOf("=");
  return equals < 0 ? "" : trimmed.slice(0, equals).trim();
}

function validateUpdates(updates: DotenvUpdates): void {
  for (const [key, value] of Object.entries(updates)) {
    if (!/^[A-Z][A-Z0-9_]*$/.test(key)) {
      throw new Error(`environment name must be an UPPER_SNAKE identifier: ${key}`);
    }
    if (value !== null && /[\r\n\0]/.test(value)) {
      throw new Error(`${key} must be one line and cannot contain NUL bytes`);
    }
  }
}

/**
 * Atomically set/remove several variables. A null value removes every
 * assignment. Returns the names that existed before this transaction.
 */
export function updateDotenv(
  path: string,
  updates: DotenvUpdates,
  protect: ProtectSecretFile = lockDownFile,
): ReadonlySet<string> {
  validateUpdates(updates);
  const names = new Set(Object.keys(updates));
  return withDotenvLock(path, () => {
    const lines = readDotenvLines(path);
    const existed = new Set(lines.map(envName).filter((name) => names.has(name)));
    if (existed.size === 0 && Object.values(updates).every((value) => value === null)) {
      return existed;
    }
    const kept = lines.filter((line) => !names.has(envName(line)) && line.trim() !== "");
    for (const [key, value] of Object.entries(updates)) {
      if (value !== null) kept.push(`${key}=${value}`);
    }
    writeDotenv(path, kept, protect);
    return existed;
  });
}

function writeDotenv(path: string, lines: readonly string[], protect: ProtectSecretFile): void {
  const temporary = join(dirname(path), `${basename(path)}.tmp.${process.pid}.${randomUUID()}`);
  let fd: number | undefined;
  try {
    // An empty file gets its owner-only ACL before any secret byte exists.
    fd = openSync(temporary, "wx", 0o600);
    protect(temporary);
    writeFileSync(fd, `${lines.join("\n")}\n`, "utf8");
    fsyncSync(fd);
    closeSync(fd);
    fd = undefined;
    renameSync(temporary, path);
  } catch (error) {
    if (fd !== undefined) {
      try {
        closeSync(fd);
      } catch {
        // Preserve the original persistence/ACL error.
      }
    }
    rmSync(temporary, { force: true });
    throw error;
  }
}
