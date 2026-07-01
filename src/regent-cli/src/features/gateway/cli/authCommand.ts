// `regent auth status|revoke` — read/edit the gateway's pairing state in
// $REGENT_HOME/gateway-auth.json. Pairing itself happens over chat against a
// running gateway (pairing codes); this surfaces and prunes the result.
import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { out, printError } from "@app/cli/runtime.ts";
import { regentHome } from "@shared/infrastructure/deacon/locate.ts";
import { style } from "@shared/ui/style.ts";

interface AuthSnapshot {
  allow_all?: boolean;
  allowlist?: string[];
  paired?: string[];
}

const authPath = (home: string): string => join(home, "gateway-auth.json");

function readSnapshot(home: string): AuthSnapshot | null {
  try {
    return JSON.parse(readFileSync(authPath(home), "utf8")) as AuthSnapshot;
  } catch {
    return null;
  }
}

export function authCommand(profile: string, args: string[]): number {
  const home = regentHome(profile);
  const [sub, target] = args;

  if (sub === "revoke") {
    if (!target) {
      printError("usage: regent auth revoke <user-key>   (e.g. telegram:12345)");
      return 1;
    }
    const snap = readSnapshot(home);
    if (!snap) {
      printError("no pairing state (gateway-auth.json not found)");
      return 1;
    }
    const had = (snap.allowlist ?? []).includes(target) || (snap.paired ?? []).includes(target);
    if (!had) {
      out(style.grey(`no paired/allowed user '${target}'`));
      return 0;
    }
    snap.allowlist = (snap.allowlist ?? []).filter((u) => u !== target);
    snap.paired = (snap.paired ?? []).filter((u) => u !== target);
    mkdirSync(home, { recursive: true });
    const tmp = join(home, `gateway-auth.json.tmp.${process.pid}`);
    writeFileSync(tmp, JSON.stringify(snap, null, 2));
    renameSync(tmp, authPath(home));
    out(`revoked ${style.teal(target)}`);
    out(
      style.grey(
        "(restart the gateway to apply; operators also come from REGENT_TELEGRAM_ALLOWED_USERS at boot)",
      ),
    );
    return 0;
  }

  // Default: status.
  const snap = readSnapshot(home);
  if (!snap) {
    out(style.grey("no pairing state yet (gateway hasn't run, or no devices paired)"));
    return 0;
  }
  out(style.heading("Gateway auth"));
  out(`  ${"allow all".padEnd(12)} ${snap.allow_all ? style.gold("yes") : "no"}`);
  const operators = snap.allowlist ?? [];
  const paired = snap.paired ?? [];
  out(`  ${"operators".padEnd(12)} ${operators.length === 0 ? style.grey("none") : ""}`);
  for (const u of operators) out(`    ${style.teal(u)}`);
  out(`  ${"paired".padEnd(12)} ${paired.length === 0 ? style.grey("none") : ""}`);
  for (const u of paired) out(`    ${style.teal(u)}`);
  return 0;
}
