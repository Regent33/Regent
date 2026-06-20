import { expect, test } from "bun:test";
import { extractProfile, parseFlags } from "./args.ts";

test("extractProfile pulls -p, --profile, and --profile=, leaving the rest", () => {
  expect(extractProfile(["-p", "work", "model", "set", "x"])).toEqual({
    profile: "work",
    rest: ["model", "set", "x"],
  });
  expect(extractProfile(["sessions", "--profile=home", "list"])).toEqual({
    profile: "home",
    rest: ["sessions", "list"],
  });
  expect(extractProfile(["doctor"])).toEqual({ profile: "", rest: ["doctor"] });
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

test("parseFlags ignores unknown flags", () => {
  const r = parseFlags(["list", "--bogus", "x", "--limit", "5"], { limit: { type: "string" } });
  expect(r.values).toEqual({ limit: "5" });
  expect(r.positionals).toEqual(["list", "x"]);
});
