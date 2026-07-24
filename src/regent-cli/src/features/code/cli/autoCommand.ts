// `regent auto [on|off]` — a top-level shortcut for auto mode, the SAME
// `tools.auto_approve` setting as `regent code settings auto` and the desktop
// app's Safety toggle. Set through the deacon's validated `config.set`, so the
// change is schema-checked and applies live (open sessions AND the gateway,
// which reads tools.auto_approve per approval). Bare `regent auto` shows state.
import { out, printError, withClient } from "@app/cli/runtime.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { style } from "@shared/ui/style.ts";

export async function autoCommand(profile: string, args: readonly string[]): Promise<number> {
  const [sub, ...extra] = args;
  if (sub === undefined) {
    return withClient(profile, showAuto);
  }
  if ((sub === "on" || sub === "off") && extra.length === 0) {
    return withClient(profile, (client) => setAuto(client, sub === "on"));
  }
  printError("usage: regent auto [on|off]");
  return 1;
}

async function showAuto(client: IRpcClient): Promise<number> {
  const res = await client.call<{ tools?: { auto_approve?: boolean } }>("config.get", {}, 30_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  const on = res.value.tools?.auto_approve === true;
  out(`auto mode: ${style.value(on ? "on" : "off")}`);
  out(
    style.grey(
      "approve every tool action without prompting — applies live to open sessions and the gateway",
    ),
  );
  out(style.grey("change with: regent auto on|off"));
  return 0;
}

async function setAuto(client: IRpcClient, enabled: boolean): Promise<number> {
  const res = await client.call<{ note: string }>(
    "config.set",
    { path: "tools.auto_approve", value: enabled },
    30_000,
  );
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  out(`auto mode ${style.value(enabled ? "on" : "off")}`);
  if (enabled) {
    out(
      style.warn(
        "  ⚠ Regent will now run dangerous actions without asking. Turn off: regent auto off",
      ),
    );
  }
  return 0;
}
