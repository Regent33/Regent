// Tiny argv helpers for the CLI router — enough for the subcommand set without
// pulling in a parser dependency.

/**
 * True when a token is an option rather than a positional. A leading `-` is not
 * enough on its own: `-` means stdin, `--` ends option parsing, and `-3` is a
 * negative number. Getting this wrong is how "unknown option" ends up reported
 * as "unknown command".
 */
export function looksLikeOption(token: string): boolean {
  if (!token.startsWith("-") || token === "-" || token === "--") return false;
  return !/^-\d/.test(token);
}

export interface GlobalArgs {
  readonly profile: string;
  readonly rest: string[];
  /**
   * True when a `--` preceded the command token, so a command that starts with
   * a dash is a literal positional rather than a mistyped option. It says
   * nothing about subcommand argv — subcommands do their own parsing and a
   * terminator inside them is not plumbed through (nothing needs it yet).
   */
  readonly commandIsLiteral: boolean;
  /** A usage error in the global options; the caller must not continue. */
  readonly error: string | null;
}

/** Pull the global -p/--profile flag out of argv, returning it and the rest. */
export function extractProfile(argv: readonly string[]): GlobalArgs {
  const rest: string[] = [];
  let profile = "";
  let afterTerminator = false;
  let commandIsLiteral = false;
  const fail = (error: string): GlobalArgs => ({
    profile: "",
    rest: [],
    commandIsLiteral: false,
    error,
  });

  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === undefined) continue;
    if (afterTerminator) {
      rest.push(a);
      continue;
    }
    if (a === "--") {
      afterTerminator = true;
      // Only a terminator seen before the command makes the command literal.
      if (rest.length === 0) commandIsLiteral = true;
      continue;
    }
    if (a === "-p" || a === "--profile") {
      // Swallowing the next token blindly turned `regent --profile --help`
      // into a profile named "--help" and opened chat instead of printing help.
      const value = argv[i + 1];
      if (value === undefined || looksLikeOption(value)) {
        return fail(`${a} requires a profile name`);
      }
      profile = value;
      i++;
      continue;
    }
    if (a.startsWith("--profile=")) {
      const value = a.slice("--profile=".length);
      if (value === "") return fail("--profile= requires a profile name");
      profile = value;
      continue;
    }
    rest.push(a);
  }
  return { profile, rest, commandIsLiteral, error: null };
}

/**
 * Option-looking tokens in `args` that `spec` does not declare. `parseFlags`
 * ignores those silently, which turns `doctor --strcit` into a quiet non-strict
 * run — the command reports success and the typo is never mentioned.
 */
export function unknownFlags(args: readonly string[], spec: Record<string, FlagSpec>): string[] {
  const byAlias: Record<string, FlagSpec> = {};
  for (const s of Object.values(spec)) if (s.alias) byAlias[s.alias] = s;
  const unknown: string[] = [];
  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    if (a === undefined) continue;
    if (a === "--") break; // everything after the terminator is a positional
    if (!looksLikeOption(a)) continue;
    const long = a.startsWith("--");
    const inline = long && a.includes("=");
    const name = long ? a.slice(2).split("=")[0] : a.slice(1);
    const found = name === undefined ? undefined : long ? spec[name] : byAlias[name];
    if (found === undefined) {
      unknown.push(a);
      continue;
    }
    // A declared string flag eats the next token, which may itself look like an
    // option (`--prompt --weird`); that is its value, not an unknown flag.
    if (found.type === "string" && !inline) i++;
  }
  return unknown;
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
