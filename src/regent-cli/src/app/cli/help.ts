import { EXIT } from "@app/cli/exit.ts";
import { err, out, printError } from "@app/cli/runtime.ts";
// `regent version` and `regent help`.
import { BRAND } from "@app/config/brand.ts";
import { CLI_COMMAND_GROUPS, COMMANDS_BY_NAME } from "@app/config/commands.ts";
import { style } from "@shared/ui/style.ts";

export const CLI_VERSION = BRAND.version;

export function printVersion(): number {
  out(`regent ${CLI_VERSION}`);
  return 0;
}

const id = <T>(s: T) => s;

// The full help text, built from CLI_COMMAND_GROUPS + COMMAND_HELP (single
// source). `color` paints it for the shell (`regent help`); plain text is used
// for the in-chat `/help` note (embedded ANSI would corrupt Ink's layout).
function helpLines(color: boolean): string[] {
  const head = color ? style.heading : id<string>;
  const grp = color ? style.teal : id<string>;
  const dim = color ? style.grey : id<string>;
  const bold = color ? style.bold : id<string>;
  const lines: string[] = [
    `${bold("regent")} — ${BRAND.tagline}`,
    "",
    head("Usage"),
    "  regent [chat]            interactive chat (default)",
    "  regent <command> [args]",
    "",
    `${head("Commands")}  ${dim("— run from your shell as: regent <command> [args]")}`,
  ];
  for (const [group, names] of Object.entries(CLI_COMMAND_GROUPS)) {
    lines.push(grp(`  ${group}`));
    for (const name of names) {
      lines.push(`    ${name.padEnd(10)} ${dim(COMMANDS_BY_NAME[name]?.summary ?? "")}`);
    }
  }
  lines.push(
    "",
    `${head("In chat")}  ${dim("— type these inside a session")}`,
    `  ${"/help".padEnd(10)} ${dim("show this help")}`,
    `  ${"/doctor".padEnd(10)} ${dim("check provider / model / API-key (diagnose 401s)")}`,
    `  ${"/new".padEnd(10)} ${dim("clear the transcript (also /clear)")}`,
    `  ${"/stop".padEnd(10)} ${dim("interrupt the running turn")}`,
    `  ${"/approve".padEnd(10)} ${dim("approve a pending sensitive action (also /deny)")}`,
    `  ${"/quit".padEnd(10)} ${dim("leave Regent (also /exit)")}`,
    dim("  …plus any command above, prefixed with / — e.g. /status, /kanban list, /soul, /persona"),
    "",
    dim("Global:  -p, --profile <name>  isolate state under a profile"),
  );
  return lines;
}

/** Plain-text help for the in-chat `/help` note. */
export function helpText(): string {
  return helpLines(false).join("\n");
}

/**
 * Print the full help. `write` picks the stream: stdout when help is what was
 * asked for, stderr when it accompanies a usage error.
 */
export function printHelp(write: (s: string) => void = out): number {
  write(helpLines(true).join("\n"));
  return 0;
}

/**
 * The unrecognised-invocation path. Diagnosing an option as an option matters:
 * reporting `--nosuchopt` as an "unknown command" sends the reader looking for
 * a subcommand that never existed. Everything goes to stderr — a failed
 * invocation must leave stdout empty for whoever is reading the pipe.
 */
export function printUnknown(command: string, isOption: boolean): number {
  printError(`${isOption ? "unknown option" : "unknown command"}: ${command}`);
  err("");
  printHelp(err);
  return EXIT.usage;
}

/**
 * Full help for one command, rendered from its spec entry (C.2). This used to
 * print a single line and no flags at all — `regent code --help` told you
 * nothing about `--yes`. It still answers locally: a stuck deacon must never be
 * able to hang the help text.
 */
export function printCommandHelp(command: string): number {
  const spec = COMMANDS_BY_NAME[command];
  if (!spec) {
    out(style.bold(`regent ${command}`));
    out(style.grey("Run `regent help` for the full command list."));
    return 0;
  }
  const pad = (rows: ReadonlyArray<readonly [string, string]>) => {
    const w = Math.max(...rows.map(([k]) => k.length));
    for (const [k, v] of rows) out(`  ${k.padEnd(w)}  ${style.grey(v)}`);
  };

  out(`${style.bold(`regent ${spec.name}`)} — ${spec.summary}`);
  out("");
  out(style.heading("Usage"));
  for (const line of spec.usage ?? [`regent ${spec.name}`]) out(`  ${line}`);
  if (spec.subcommands) {
    out("");
    out(style.heading("Subcommands"));
    pad(spec.subcommands);
  }
  if (spec.flags) {
    out("");
    out(style.heading("Flags"));
    pad(
      spec.flags.map(
        (f) =>
          [
            `--${f.name}${f.alias ? `, -${f.alias}` : ""}${f.type === "string" ? " <value>" : ""}`,
            f.doc,
          ] as const,
      ),
    );
  }
  if (spec.examples) {
    out("");
    out(style.heading("Examples"));
    for (const e of spec.examples) out(`  ${e}`);
  }
  if (spec.needsDeacon) {
    out("");
    out(style.grey("Needs a running deacon — `regent doctor` if it will not start."));
  }
  if (spec.seeAlso)
    out(style.grey(`See also: ${spec.seeAlso.map((s) => `regent ${s}`).join(" · ")}`));
  return 0;
}
