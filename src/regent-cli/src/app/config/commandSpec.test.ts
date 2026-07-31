// The point of one specification is that it cannot drift from the router. These
// tests read the router's own source and compare, so a command added to one and
// not the other fails CI instead of being quietly undocumented.
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { COMMAND_SPECS, COMMANDS_BY_NAME } from "./commands.ts";

const routerSource = readFileSync(join(import.meta.dir, "../cli/router.ts"), "utf8");

/** Every `case "x":` the router dispatches on. */
function routerCases(): string[] {
  return [...routerSource.matchAll(/case "([^"]*)":/g)].map((m) => m[1] as string);
}

// Aliases and non-user-facing entries the help surface deliberately omits.
const NOT_COMMANDS = new Set([
  "", // bare `regent` — documented as `chat`
  "job", // alias of jobs
  "agent", // alias of agents
  "__render", // hidden sidecar for create_document
  "--version",
  "-v",
  "--help",
  "-h",
  "help",
]);

test("every command the router dispatches has a spec entry", () => {
  const undocumented = routerCases().filter(
    (c) => !NOT_COMMANDS.has(c) && COMMANDS_BY_NAME[c] === undefined,
  );
  expect(undocumented).toEqual([]);
});

test("every spec entry is a command the router actually dispatches", () => {
  const cases = new Set(routerCases());
  const phantom = COMMAND_SPECS.map((s) => s.name).filter((n) => !cases.has(n));
  expect(phantom).toEqual([]);
});

test("no duplicate names, and every entry has a group and a summary", () => {
  const names = COMMAND_SPECS.map((s) => s.name);
  expect(new Set(names).size).toBe(names.length);
  for (const s of COMMAND_SPECS) {
    expect(s.group).not.toBe("");
    expect(s.summary.trim()).not.toBe("");
  }
});

test("flags are documented in the form the parser accepts", () => {
  for (const s of COMMAND_SPECS) {
    for (const f of s.flags ?? []) {
      // Long names are matched literally by parseFlags after `--`, so a leading
      // dash or a space in the name would document a flag that cannot be typed.
      expect(f.name).toMatch(/^[a-z][a-z0-9-]*$/);
      expect(f.alias === undefined || /^[a-zA-Z]$/.test(f.alias)).toBe(true);
      expect(f.doc.trim()).not.toBe("");
    }
  }
});

test("examples are syntactically valid invocations of their own command", () => {
  // Deliberately NOT "runnable": examples may need credentials, a session or a
  // network. Syntax is what CI can honestly check.
  for (const s of COMMAND_SPECS) {
    for (const e of s.examples ?? []) {
      // `… | regent ask` is a legitimate shape, so "starts with regent" is too
      // strict — but the example still has to invoke regent somewhere.
      expect(e.startsWith("regent") || e.includes("| regent")).toBe(true);
      // Balanced quotes — an unbalanced one is a copy-paste trap.
      expect((e.match(/"/g) ?? []).length % 2).toBe(0);
      expect((e.match(/'/g) ?? []).length % 2).toBe(0);
    }
  }
});

test("see-also targets exist", () => {
  for (const s of COMMAND_SPECS) {
    for (const target of s.seeAlso ?? []) {
      expect(COMMANDS_BY_NAME[target]).toBeDefined();
    }
  }
});
