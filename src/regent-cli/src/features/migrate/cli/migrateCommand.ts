// `regent migrate` — import an existing Hermes or OpenClaw install into
// Regent. Dry-run by default (prints exactly what would happen); `--apply`
// writes. Additive only: the source is never modified, and existing Regent
// skills are never overwritten.
import { cpSync, existsSync, readdirSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { parseFlags } from "@app/cli/args.ts";
import { out, printError } from "@app/cli/runtime.ts";
import { regentHome } from "@shared/infrastructure/deacon/locate.ts";
import { style } from "@shared/ui/style.ts";

export function migrateCommand(profile: string, args: string[]): number {
  const source = args[0];
  const { values } = parseFlags(args.slice(1), {
    home: { type: "string" },
    apply: { type: "boolean" },
  });
  const apply = values.apply === true;
  switch (source) {
    case "hermes":
      return migrateHermes(
        typeof values.home === "string" ? values.home : join(homedir(), ".hermes"),
        regentHome(profile),
        apply,
      );
    case "openclaw":
      return migrateOpenclaw(
        typeof values.home === "string" ? values.home : join(homedir(), ".openclaw"),
      );
    default:
      printError("usage: regent migrate <hermes|openclaw> [--home <path>] [--apply]");
      out(style.grey("  dry-run by default — shows the plan; --apply performs the import."));
      return 1;
  }
}

/** A Hermes skill dir: contains SKILL.md directly, or nests skills one
 *  category level down (Hermes ships skills/<category>/<skill>/SKILL.md). */
function findHermesSkills(skillsDir: string): { name: string; dir: string }[] {
  const found: { name: string; dir: string }[] = [];
  if (!existsSync(skillsDir)) return found;
  for (const entry of readdirSync(skillsDir, { withFileTypes: true })) {
    if (!entry.isDirectory() || entry.name.startsWith(".")) continue;
    const direct = join(skillsDir, entry.name);
    if (existsSync(join(direct, "SKILL.md"))) {
      found.push({ name: entry.name, dir: direct });
      continue;
    }
    for (const nested of readdirSync(direct, { withFileTypes: true })) {
      const dir = join(direct, nested.name);
      if (nested.isDirectory() && existsSync(join(dir, "SKILL.md")))
        found.push({ name: nested.name, dir });
    }
  }
  return found;
}

function migrateHermes(hermesHome: string, home: string, apply: boolean): number {
  if (!existsSync(hermesHome)) {
    printError(`no Hermes install at ${hermesHome} (override with --home <path>)`);
    return 1;
  }
  out(`${style.heading("Migrate from Hermes")} ${style.grey(hermesHome)}\n`);

  // Skills: Hermes categories flatten into Regent's skills/<name>/ layout
  // (both are agentskills.io SKILL.md folders, so the content copies as-is).
  const skills = findHermesSkills(join(hermesHome, "skills"));
  const targetSkills = join(home, "skills");
  const fresh = skills.filter((s) => !existsSync(join(targetSkills, s.name)));
  const skipped = skills.length - fresh.length;
  out(
    `  skills: ${style.teal(String(fresh.length))} to import → ${targetSkills}${skipped ? style.grey(`  (${skipped} already exist — kept, not overwritten)`) : ""}`,
  );

  // What this importer does NOT cover yet — say so instead of half-doing it.
  for (const [what, file] of [
    ["memories", join(hermesHome, "memories")],
    ["session history", join(hermesHome, "state.db")],
    ["cron jobs", join(hermesHome, "cron")],
    ["config", join(hermesHome, "config.yaml")],
  ] as const) {
    if (existsSync(file))
      out(style.grey(`  ${what}: found but not imported yet — say the word and I'll add it`));
  }

  if (!apply) {
    out(
      `\n${style.grey("dry run — re-run with")} ${style.teal("--apply")} ${style.grey("to import.")}`,
    );
    return 0;
  }
  let copied = 0;
  for (const s of fresh) {
    try {
      cpSync(s.dir, join(targetSkills, s.name), { recursive: true, errorOnExist: false });
      copied++;
    } catch (e) {
      printError(`  ${s.name}: ${(e as Error).message}`);
    }
  }
  out(
    `\n${style.pass("✓")} imported ${copied}/${fresh.length} skills — restart the deacon (or \`regent status\`) to pick them up.`,
  );
  return copied === fresh.length ? 0 : 1;
}

function migrateOpenclaw(openclawHome: string): number {
  if (!existsSync(openclawHome)) {
    printError(`no OpenClaw install at ${openclawHome} (override with --home <path>)`);
    return 1;
  }
  // ponytail: detection only — no OpenClaw install exists here to map against.
  // When one does: workspace memory files → memory, SOUL/AGENTS.md → persona,
  // config → config.yaml. Report what's present so the user knows the shape.
  out(`${style.heading("Migrate from OpenClaw")} ${style.grey(openclawHome)}\n`);
  for (const name of readdirSync(openclawHome).slice(0, 20)) out(`  found: ${name}`);
  out(
    `\n${style.warn("⚠")} the OpenClaw importer isn't implemented yet — this listing is what it would map. Ask and I'll build it against your data.`,
  );
  return 1;
}
