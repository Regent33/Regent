// Tiny argv helpers for the CLI router — enough for the subcommand set without
// pulling in a parser dependency.

/** Pull the global -p/--profile flag out of argv, returning it and the rest. */
export function extractProfile(argv: readonly string[]): { profile: string; rest: string[] } {
  const rest: string[] = [];
  let profile = "";
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === undefined) continue;
    if (a === "-p" || a === "--profile") {
      profile = argv[i + 1] ?? "";
      i++;
      continue;
    }
    if (a.startsWith("--profile=")) {
      profile = a.slice("--profile=".length);
      continue;
    }
    rest.push(a);
  }
  return { profile, rest };
}

export interface FlagSpec {
  readonly type: "string" | "boolean";
  readonly alias?: string;
}

/** Parse long (--name, --name=v, --name v) and aliased short (-x) flags. */
export function parseFlags(
  args: readonly string[],
  spec: Record<string, FlagSpec>,
): { values: Record<string, string | boolean>; positionals: string[] } {
  const byAlias: Record<string, string> = {};
  for (const [name, s] of Object.entries(spec)) if (s.alias) byAlias[s.alias] = name;

  const values: Record<string, string | boolean> = {};
  const positionals: string[] = [];
  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    if (a === undefined) continue;
    if (a.startsWith("--")) {
      const eq = a.indexOf("=");
      const name = eq >= 0 ? a.slice(2, eq) : a.slice(2);
      const s = spec[name];
      if (!s) continue;
      if (s.type === "boolean") values[name] = true;
      else if (eq >= 0) values[name] = a.slice(eq + 1);
      else {
        values[name] = args[i + 1] ?? "";
        i++;
      }
      continue;
    }
    if (a.length > 1 && a.startsWith("-")) {
      const name = byAlias[a.slice(1)];
      const s = name ? spec[name] : undefined;
      if (!name || !s) continue;
      if (s.type === "boolean") values[name] = true;
      else {
        values[name] = args[i + 1] ?? "";
        i++;
      }
      continue;
    }
    positionals.push(a);
  }
  return { values, positionals };
}
