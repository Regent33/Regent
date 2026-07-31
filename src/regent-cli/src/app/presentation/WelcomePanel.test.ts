// The welcome panel's left column must never be wider than the space beside
// the king mark. When it was, Ink wrapped the line into the gap and the art
// came out mangled — reported as "the text pushes the king art".
//
// Pure functions, so this needs no terminal.
import { expect, test } from "bun:test";
import { fitCategory, leftColumnBudget, truncate } from "./WelcomePanel.tsx";

const KING = 30;
const width = (columns: number) => columns - 2; // what WelcomePanel passes to Panel

/** What one line actually occupies once rendered. */
const rendered = (c: string, items: string[], budget: number) => {
  const { label, body } = fitCategory(c, items, budget);
  return label.length + body.length;
};

test("a line never exceeds its budget, whatever the category name costs", () => {
  const budget = leftColumnBudget(width(230), KING);
  // The exact case from the bug report: seven long skill names under a
  // category whose own name is 7 characters. The old code budgeted the items
  // and forgot it also prints "general: " and " …".
  const general = [
    "background-task-verification",
    "browser-rendering-debugging",
    "evidence-first-background-tasks",
    "find_physics_diagrams",
    "regent-artifacts-cleanup",
    "regent-project-layout",
    "regent-tool-invocation",
  ];
  expect(rendered("general", general, budget)).toBeLessThanOrEqual(budget);
  // Any category name at all — the overflow used to start at 7 characters.
  for (const name of ["a", "core", "general", "debugging", "implementation", "x".repeat(40)]) {
    expect(rendered(name, general, budget)).toBeLessThanOrEqual(budget);
  }
});

test("one item wider than the whole column is truncated, not emitted intact", () => {
  const budget = leftColumnBudget(width(100), KING);
  // The old loop admitted the first item unconditionally, so a single long
  // name blew the line open by itself.
  expect(rendered("tools", ["x".repeat(400)], budget)).toBeLessThanOrEqual(budget);
});

test("across terminal widths, including absurdly narrow ones", () => {
  for (const columns of [40, 60, 80, 100, 140, 200, 230, 400]) {
    const budget = leftColumnBudget(width(columns), KING);
    const line = rendered("implementation", ["alpha-tool", "beta-tool", "gamma-tool"], budget);
    expect(line).toBeLessThanOrEqual(budget);
  }
});

test("the budget leaves the king mark its full width", () => {
  // budget + panel chrome + gap + art must fit inside the panel.
  const columns = 230;
  const w = width(columns);
  expect(leftColumnBudget(w, KING) + 4 + 4 + KING).toBeLessThanOrEqual(w);
});

test("short content is left whole — no gratuitous ellipsis", () => {
  const budget = leftColumnBudget(width(200), KING);
  const { label, body } = fitCategory("core", ["current_time"], budget);
  expect(label).toBe("core: ");
  expect(body).toBe("current_time");
});

test("an ellipsis appears exactly when something was dropped", () => {
  const budget = 30;
  expect(fitCategory("core", ["a", "b"], budget).body).toBe("a, b");
  expect(fitCategory("core", ["a".repeat(20), "b".repeat(20)], budget).body).toEndWith("…");
});

test("no items renders nothing rather than a stray ellipsis", () => {
  expect(fitCategory("core", [], 40)).toEqual({ label: "core: ", body: "" });
});

// Everything printed under the art shares the art's 30-column width. A line
// wider than that wraps, the column grows taller, and the panel goes out of
// balance — which is what a real 40-character model id did.
test("nothing under the king mark is wider than the mark", () => {
  const KING_W = 30;
  const lines = [
    truncate("nvidia/nvidia/nemotron-3-ultra-550b-a55b", KING_W), // the real one
    truncate("D:\\1-1@k\\@ServeAI\\Regent\\src\\regent-app\\Desktop", KING_W),
    `session ${truncate("sess_041e5a1e202f54031cf1aaaaaaaaaa", KING_W - 8)}`,
    truncate("claude-opus-5", KING_W), // short ones pass through untouched
  ];
  for (const line of lines) expect(line.length).toBeLessThanOrEqual(KING_W);
  expect(lines[3]).toBe("claude-opus-5");
  // The TAIL survives: the identifying half of a model id and of a path.
  expect(lines[0]).toEndWith("550b-a55b");
  expect(lines[0]).toStartWith("…");
});
