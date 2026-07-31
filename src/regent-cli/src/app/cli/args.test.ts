import { expect, test } from "bun:test";
import { extractProfile, looksLikeOption, parseFlags, unknownFlags } from "./args.ts";

const ok = (profile: string, rest: string[], commandIsLiteral = false) => ({
  profile,
  rest,
  commandIsLiteral,
  error: null,
});

test("extractProfile pulls -p, --profile, and --profile=, leaving the rest", () => {
  expect(extractProfile(["-p", "work", "model", "set", "x"])).toEqual(
    ok("work", ["model", "set", "x"]),
  );
  expect(extractProfile(["sessions", "--profile=home", "list"])).toEqual(
    ok("home", ["sessions", "list"]),
  );
  expect(extractProfile(["doctor"])).toEqual(ok("", ["doctor"]));
});

test("`--` ends option parsing and everything after it stays literal", () => {
  expect(extractProfile(["--", "-p", "work"])).toEqual(ok("", ["-p", "work"], true));
  // The flag before the terminator is still a flag.
  expect(extractProfile(["-p", "work", "--", "--profile=nope"])).toEqual(
    ok("work", ["--profile=nope"], true),
  );
  // A terminator AFTER the command does not make the command literal: the
  // mistyped option before it must still be diagnosed as an option.
  expect(extractProfile(["--nosuchopt", "--", "x"])).toEqual(ok("", ["--nosuchopt", "x"]));
});

test("a --profile without a value is a usage error, not a profile named --help", () => {
  for (const argv of [["-p"], ["--profile"], ["--profile", "--help"], ["--profile="]]) {
    const r = extractProfile(argv);
    expect(r.error).toContain("requires a profile name");
    expect(r.rest).toEqual([]);
  }
  // A profile that merely looks numeric is still a value, not an option.
  expect(extractProfile(["-p", "-5", "status"])).toEqual(ok("-5", ["status"]));
});

test("looksLikeOption separates options from stdin, terminators and numbers", () => {
  expect(looksLikeOption("--verbose")).toBe(true);
  expect(looksLikeOption("-v")).toBe(true);
  expect(looksLikeOption("-abc")).toBe(true); // short cluster
  expect(looksLikeOption("-")).toBe(false); // stdin
  expect(looksLikeOption("--")).toBe(false); // terminator
  expect(looksLikeOption("-5")).toBe(false); // negative number
  expect(looksLikeOption("-1.5")).toBe(false);
  expect(looksLikeOption("status")).toBe(false);
});

test("parseFlags handles value forms, booleans, aliases, and positionals", () => {
  const r = parseFlags(["add", "morning", "--schedule", "1d", "--prompt=hello", "-f"], {
    schedule: { type: "string" },
    prompt: { type: "string" },
    follow: { type: "boolean", alias: "f" },
  });
  expect(r.positionals).toEqual(["add", "morning"]);
  expect(r.values).toEqual({ schedule: "1d", prompt: "hello", follow: true });
});

test("unknownFlags names typo'd options that parseFlags would silently drop", () => {
  const spec = { strict: { type: "boolean" }, out: { type: "string", alias: "o" } } as const;
  expect(unknownFlags(["--strict"], spec)).toEqual([]);
  expect(unknownFlags(["-o", "file"], spec)).toEqual([]);
  expect(unknownFlags(["--out=file"], spec)).toEqual([]);
  expect(unknownFlags(["--strcit"], spec)).toEqual(["--strcit"]);
  expect(unknownFlags(["-x"], spec)).toEqual(["-x"]);
  // A string flag's value is its value even when it looks like an option.
  expect(unknownFlags(["--out", "--weird"], spec)).toEqual([]);
  // Positionals, negative numbers and everything after `--` are not options.
  expect(unknownFlags(["show", "-5", "--", "--anything"], spec)).toEqual([]);
});

test("parseFlags ignores unknown flags", () => {
  const r = parseFlags(["list", "--bogus", "x", "--limit", "5"], { limit: { type: "string" } });
  expect(r.values).toEqual({ limit: "5" });
  expect(r.positionals).toEqual(["list", "x"]);
});
