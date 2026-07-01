import { expect, test } from "bun:test";
// Integration smoke: a real round-trip against the built regent-deacon. This
// one does real I/O (spawns the daemon) and auto-skips when the binary isn't
// built, so unit runs stay green on a fresh checkout.
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { locateDeacon } from "./locate.ts";
import { connectDeacon } from "./spawn.ts";

const located = locateDeacon();
const itOrSkip = located.ok ? test : test.skip;

itOrSkip(
  "health round-trips against the real daemon",
  async () => {
    if (!located.ok) return;
    const home = mkdtempSync(join(tmpdir(), "regent-cli-smoke-"));
    const connected = connectDeacon(located.value, home);
    expect(connected.ok).toBe(true);
    if (!connected.ok) return;

    const client = connected.value;
    const res = await client.call("health", {}, 15_000);
    await client.close();
    expect(res.ok).toBe(true);
  },
  20_000,
);
