import { expect, test } from "bun:test";
// Integration smoke: a real round-trip against the built regent-daemon. This
// one does real I/O (spawns the daemon) and auto-skips when the binary isn't
// built, so unit runs stay green on a fresh checkout.
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { locateDaemon } from "./locate.ts";
import { connectDaemon } from "./spawn.ts";

const located = locateDaemon();
const itOrSkip = located.ok ? test : test.skip;

itOrSkip(
  "health round-trips against the real daemon",
  async () => {
    if (!located.ok) return;
    const home = mkdtempSync(join(tmpdir(), "regent-tui-smoke-"));
    const connected = connectDaemon(located.value, home);
    expect(connected.ok).toBe(true);
    if (!connected.ok) return;

    const client = connected.value;
    const res = await client.call("health", {}, 15_000);
    await client.close();
    expect(res.ok).toBe(true);
  },
  20_000,
);
