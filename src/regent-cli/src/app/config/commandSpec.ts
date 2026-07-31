// ONE specification of the command surface (plan C.1).
//
// Before this there were three: the group table for the welcome panel, a
// one-line-description map for `regent help`, and the router's switch. Nothing
// tied them together, so a command could exist in the router and be invisible in
// help, or be described in help with flags the parser never accepted.
//
// Everything now reads this: `regent help`, `regent <cmd> --help`, the welcome
// panel, and the shell completions. `commandSpec.test.ts` asserts it against the
// router's own cases, so a command added to one and not the other fails CI.

export interface FlagDoc {
  readonly name: string;
  readonly alias?: string;
  readonly type: "string" | "boolean";
  readonly doc: string;
}

export interface CommandSpec {
  readonly name: string;
  readonly group: string;
  /** One line, shown in the command list. */
  readonly summary: string;
  /** Extra usage lines beyond `regent <name>`. */
  readonly usage?: readonly string[];
  /** Nested verbs. `memory pending` needs its own line, not just parent help. */
  readonly subcommands?: ReadonlyArray<readonly [string, string]>;
  /** Transcribed from the command's own parseFlags spec — never invented. */
  readonly flags?: readonly FlagDoc[];
  readonly examples?: readonly string[];
  readonly seeAlso?: readonly string[];
  /** The router health-checks a deacon before dispatching this one. */
  readonly needsDeacon?: boolean;
}

/** Index by name, for the help renderer and the completion generator. */
export function byName(specs: readonly CommandSpec[]): Record<string, CommandSpec> {
  const map: Record<string, CommandSpec> = {};
  for (const s of specs) map[s.name] = s;
  return map;
}

/** Group → names, in declaration order, for the help listing. */
export function groups(specs: readonly CommandSpec[]): Record<string, readonly string[]> {
  const out: Record<string, string[]> = {};
  for (const s of specs) {
    const bucket = out[s.group] ?? [];
    bucket.push(s.name);
    out[s.group] = bucket;
  }
  return out;
}
