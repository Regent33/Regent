import { expect, test } from "bun:test";
import { SLASH_COMMANDS, matchSlash } from "./commands.ts";

test("matchSlash opens only on a bare /prefix and filters by name", () => {
  expect(matchSlash("hello")).toBeNull(); // no leading slash → no menu
  expect(matchSlash("/voice status")).toBeNull(); // a space → past the command word
  expect(matchSlash("/")).toEqual(SLASH_COMMANDS); // bare slash lists everything
  expect(matchSlash("/vo")?.map((c) => c.name)).toEqual(["voice"]);
  expect(matchSlash("/St")?.map((c) => c.name)).toContain("status"); // case-insensitive
  expect(matchSlash("/zzz")).toEqual([]); // no match → empty (menu stays closed)
});

test("every slash command has a non-empty description", () => {
  for (const c of SLASH_COMMANDS) {
    expect(c.name).toMatch(/^[a-z]+$/);
    expect(c.description.length).toBeGreaterThan(0);
  }
});
