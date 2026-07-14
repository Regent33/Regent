// Mounts the Ink setup wizard: fetch the supported-provider catalog from the
// deacon, run the staged pickers, persist the choice through the same
// writeSetup path as the flag-driven flow, and print the familiar summary.
import { render } from "ink";
import { mkdirSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
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
    const app = render(
      <SetupWizard
        catalog={catalog.value}
        defaultHome={regentHome(profile)}
        onDone={(r) => (result = r)}
      />,
    );
    await app.waitUntilExit();

    const picked = result as WizardResult | null;
    if (picked === null) {
      out(style.grey("setup cancelled — run `regent setup` anytime"));
      return 1;
    }

    const home = picked.home;
    // A non-default data directory sticks via `~/.regent/.home` (read by
    // regentHome before config exists). REGENT_HOME env / -p still win —
    // don't write a pointer those would silently override.
    if (!profile && home !== regentHome(profile)) {
      if (process.env.REGENT_HOME) {
        out(
          style.warn(
            `REGENT_HOME is set in your environment (${process.env.REGENT_HOME}) — it overrides this choice; unset it or update it to ${home}`,
          ),
        );
      } else {
        const def = join(homedir() || ".", ".regent");
        mkdirSync(def, { recursive: true });
        writeFileSync(join(def, ".home"), `${home}\n`);
      }
    }
    mkdirSync(home, { recursive: true });
    writeEnv(home, picked.key);
    writeConfig(home, picked.provider, picked.model, "", true);
    markSetupDone(home);

    // The self-introduction lands in the `about` persona row — the same
    // profile the agent renders into every session's prompt.
    let firstName = "";
    if (picked.about !== "") {
      const saved = await client.call("persona.set", { key: "about", content: picked.about }, 15_000);
      if (!saved.ok) out(style.warn(`could not save your intro: ${saved.error.message}`));
      // "I'm Sam" / "my name is Sam" / "call me Sam" — else the first
      // capitalized word that isn't the pronoun "I".
      firstName =
        picked.about.match(/(?:i'?m|i am|name(?:'s| is)?|call me)\s+([A-Za-z][\w-]*)/i)?.[1] ??
        picked.about.match(/\b(?!I\b)[A-Z][\w-]*/)?.[0] ??
        "";
    }

    out("");
    if (firstName) {
      out(`${style.teal("♚")} Alright ${style.bold(firstName)} — crown fitted, court assembled, memory primed.`);
      out(style.grey("  I'll remember what you told me. Change it anytime with `regent persona`."));
    }
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
