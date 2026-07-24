import { afterEach, expect, test } from "bun:test";
// Integration smoke: real round-trips against the built regent-deacon. These do
// real I/O (spawn the deacon) and auto-skip when the binary isn't built, so unit
// runs stay green on a fresh checkout.
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { connectHealthyDeacon } from "./connect.ts";

const EXE = process.platform === "win32" ? ".exe" : "";
// Repo target/{release,debug}/regent-deacon, relative to this file (deacon →
// infrastructure → shared → src → regent-cli → src → repo = up 6).
function repoDeacon(): string | null {
  const repo = join(import.meta.dir, "../../../../../..");
  for (const profile of ["release", "debug"]) {
    const p = join(repo, "target", profile, `regent-deacon${EXE}`);
    if (existsSync(p)) return resolve(p);
  }
  return null;
}

const real = repoDeacon();
const itOrSkip = real ? test : test.skip;

const dirs: string[] = [];
const freshHome = (): string => {
  const d = mkdtempSync(join(tmpdir(), "regent-cli-smoke-"));
  dirs.push(d);
  return d;
};
afterEach(() => {
  for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true });
});

itOrSkip(
  "health round-trips against the real deacon",
  async () => {
    if (!real) return;
    const connected = await connectHealthyDeacon(freshHome(), { candidates: [real] });
    expect(connected.ok).toBe(true);
    if (!connected.ok) return;
    expect(connected.value.path).toBe(real);
    await connected.value.client.close();
  },
  20_000,
);

itOrSkip(
  "falls through a bad first candidate to the real deacon",
  async () => {
    if (!real) return;
    // A real file that is NOT a spawnable binary — spawn throws (Windows: not a
    // valid Win32 app; POSIX: EACCES), the loop records it and falls through.
    const bogusDir = mkdtempSync(join(tmpdir(), "regent-bogus-"));
    dirs.push(bogusDir);
    const bogus = join(bogusDir, `regent-deacon${EXE}`);
    writeFileSync(bogus, "not a binary");

    const connected = await connectHealthyDeacon(freshHome(), {
      candidates: [resolve(bogus), real],
    });
    expect(connected.ok).toBe(true);
    if (!connected.ok) return;
    expect(connected.value.path).toBe(real); // the bogus pin did not win
    await connected.value.client.close();
  },
  20_000,
);
