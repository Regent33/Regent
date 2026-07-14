// Mounts the Ink setup wizard: fetch the supported-provider catalog from the
// deacon, run the staged pickers, persist the choice through the same
// writeSetup path as the flag-driven flow, and print the familiar summary.
import { render } from "ink";
import { mkdirSync } from "node:fs";
import { out, printError, withClient } from "@app/cli/runtime.ts";
import { fetchCatalog } from "@features/setup/domain/catalog.ts";
import { markSetupDone } from "@features/setup/domain/firstRun.ts";
import { writeConfig, writeEnv } from "@features/setup/domain/writeSetup.ts";
import { regentHome } from "@shared/infrastructure/deacon/locate.ts";
import { style } from "@shared/ui/style.ts";
import { SetupWizard, type WizardResult } from "../presentation/SetupWizard.tsx";

/** 0 = saved · 1 = user cancelled · 2 = wizard unavailable (no deacon /
 * catalog) — the caller falls back to the linear prompt flow on 2. */
export async function runSetupWizard(profile: string): Promise<number> {
  let ran = false;
  const code = await withClient(profile, async (client) => {
    const catalog = await fetchCatalog(client);
    if (!catalog.ok) {
      printError(`cannot load provider catalog: ${catalog.error.message}`);
      return 2;
    }
    ran = true;

    let result: WizardResult | null = null;
    const app = render(<SetupWizard catalog={catalog.value} onDone={(r) => (result = r)} />);
    await app.waitUntilExit();

    const picked = result as WizardResult | null;
    if (picked === null) {
      out(style.grey("setup cancelled — run `regent setup` anytime"));
      return 1;
    }

    const home = regentHome(profile);
    mkdirSync(home, { recursive: true });
    writeEnv(home, picked.key);
    writeConfig(home, picked.provider, picked.model, "", true);
    markSetupDone(home);

    out("");
    out(style.pass("✓ Setup complete"));
    out(`  ${style.grey("home:    ")} ${home}`);
    out(`  ${style.grey("provider:")} ${picked.provider}`);
    out(`  ${style.grey("model:   ")} ${picked.model}`);
    out(
      `  ${style.grey("api key: ")} ${picked.key ? "set" : style.warn("not set — export REGENT_API_KEY before running the agent")}`,
    );
    out("");
    out(`  Next: ${style.teal("regent doctor")}  →  ${style.teal("regent chat")}`);
    out("");
    return 0;
  });
  // withClient failing before our callback ran (no deacon binary, dead
  // health check) must not strand the user — signal "unavailable".
  return ran || code === 2 ? code : 2;
}
