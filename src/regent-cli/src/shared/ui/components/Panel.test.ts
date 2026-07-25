import { expect, test } from "bun:test";
import { panelLayout } from "./Panel.tsx";

test("panel width fills the requested inner terminal width", () => {
  expect(panelLayout("Error", 118, 120)).toEqual({ title: "Error", width: 118 });
});

test("panel width never exceeds the terminal", () => {
  expect(panelLayout("Error", 200, 80).width).toBe(79);
});

test("panel title is clipped when the terminal is narrower than the title", () => {
  const layout = panelLayout("Couldn't reach the deacon", 200, 14);
  expect(layout.width).toBe(13);
  expect(layout.title.length).toBeLessThanOrEqual(layout.width - 5);
  expect(layout.title.endsWith("…")).toBe(true);
});
