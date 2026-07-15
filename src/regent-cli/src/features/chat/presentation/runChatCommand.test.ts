// Quote handling for in-chat commands: create-style commands carry quoted,
// space-containing arguments and must reach the CLI subprocess intact.
import { expect, test } from "bun:test";
import { tokenize } from "./runChatCommand.ts";

test("plain words split on whitespace", () => {
  expect(tokenize("kanban list")).toEqual(["kanban", "list"]);
});

test("double and single quotes group and strip", () => {
  expect(
    tokenize('agents create bob --description "senior rust reviewer" --prompt "be terse"'),
  ).toEqual([
    "agents",
    "create",
    "bob",
    "--description",
    "senior rust reviewer",
    "--prompt",
    "be terse",
  ]);
  expect(tokenize("mom run research 'compare rust web frameworks'")).toEqual([
    "mom",
    "run",
    "research",
    "compare rust web frameworks",
  ]);
});

test("empty quoted arg survives and unterminated quote takes the rest", () => {
  expect(tokenize('x --flag ""')).toEqual(["x", "--flag", ""]);
  expect(tokenize('x "unterminated tail')).toEqual(["x", "unterminated tail"]);
});
