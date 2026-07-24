// `regent soul` / `regent about` — view or edit the agent persona (soul) and
// the user profile (about). Stable facets hold identity, preferences, habits,
// constraints, and goals; transient/world facts belong in memory.
import { out, printError } from "@app/cli/runtime.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { style } from "@shared/ui/style.ts";
import { getKey, keyAction } from "./personaActions.ts";

type Kind = "soul" | "about";

// Slug → heading. Must match regent-store ABOUT_SECTIONS.
const SECTIONS: ReadonlyArray<readonly [string, string]> = [
  ["identity", "Identity"],
  ["preferences", "Preferences"],
  ["habits", "Habits"],
  ["constraints", "Constraints"],
  ["goals", "Goals"],
];
const isSection = (s: string | undefined): boolean => SECTIONS.some(([slug]) => slug === s);

const LABEL: Record<Kind, string> = {
  soul: "soul (agent persona)",
  about: "about-you (your profile)",
};

const HELP = (kind: Kind): string =>
  kind === "about"
    ? 'facets: identity · preferences · habits · constraints · goals   —   regent about <facet> <set|add|edit|clear> "<text>"'
    : 'verbs: regent soul <set|add|edit|clear> "<text>"';

/** `regent persona` — view the whole persona (soul) + user profile (about). */
export async function personaShowAll(client: IRpcClient): Promise<number> {
  const soul = await getKey(client, "soul");
  if (soul === null) return 1;
  out(style.heading(LABEL.soul));
  out(soul.trim() || style.grey("(empty)"));
  out("");
  if ((await showProfile(client)) !== 0) return 1;
  out(style.grey(`\n${HELP("soul")}   (or /soul, /about in chat)`));
  return 0;
}

/** Print the full `about` profile: legacy note (if any) + the five facets. */
async function showProfile(client: IRpcClient): Promise<number> {
  out(style.heading(LABEL.about));
  const legacy = await getKey(client, "about");
  if (legacy === null) return 1;
  if (legacy.trim()) out(legacy.trim());
  let any = legacy.trim().length > 0;
  for (const [slug, heading] of SECTIONS) {
    const value = await getKey(client, `about.${slug}`);
    if (value === null) return 1;
    if (value.trim()) {
      out(style.teal(`  ${heading}`));
      out(`    ${value.trim().replace(/\n/g, "\n    ")}`);
      any = true;
    }
  }
  if (!any) out(style.grey("(empty)"));
  return 0;
}

/** Persona-profile namespaces; distinct from install-home `regent profile`. */
export async function personaProfiles(client: IRpcClient, args: string[]): Promise<number> {
  const [sub, name] = args;
  if (sub === "list") {
    const res = await client.call<{ profiles: string[]; active: string }>(
      "profile.list",
      {},
      15_000,
    );
    if (!res.ok) {
      printError(res.error.message);
      return 1;
    }
    for (const profile of res.value.profiles) {
      out(
        profile === res.value.active ? `${style.teal(profile)} ${style.grey("(active)")}` : profile,
      );
    }
    return 0;
  }
  if ((sub === "create" || sub === "switch") && name) {
    const res = await client.call(`profile.${sub}`, { name }, 15_000);
    if (!res.ok) {
      printError(res.error.message);
      return 1;
    }
    out(
      sub === "create"
        ? `created persona profile ${style.teal(name)}`
        : `active persona profile: ${style.teal(name)}`,
    );
    return 0;
  }
  printError("usage: regent persona list | create <name> | switch <name>");
  return 1;
}

export async function personaCommand(
  client: IRpcClient,
  kind: Kind,
  args: string[],
): Promise<number> {
  if (kind === "about") {
    if (isSection(args[0])) {
      const [slug, ...rest] = args;
      const heading = SECTIONS.find(([section]) => section === slug)?.[1] ?? slug;
      return keyAction(client, `about.${slug}`, `about — ${heading}`, rest);
    }
    if (args.length === 0 || args[0] === "show") {
      const code = await showProfile(client);
      if (code === 0) out(style.grey(`\n  ${HELP("about")}`));
      return code;
    }
    if (["set", "clear", "delete", "edit"].includes(args[0] ?? "")) {
      return keyAction(client, "about", LABEL.about, args);
    }
    printError(`unknown profile facet '${args[0]}'`);
    out(style.grey("  facets: identity · preferences · habits · constraints · goals"));
    return 1;
  }
  return keyAction(client, "soul", LABEL.soul, args);
}
