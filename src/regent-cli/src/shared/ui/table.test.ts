import { afterEach, expect, test } from "bun:test";
import { renderTable, visibleWidth as visible } from "@shared/ui/table.ts";

const origCols = process.stdout.columns;
afterEach(() => {
  process.stdout.columns = origCols;
});

interface Row {
  id: string;
  title: string;
}
const COLS = [
  { header: "ID", get: (r: Row) => r.id },
  { header: "TITLE", get: (r: Row) => r.title, flex: true as const },
];

test("every line shares one visible width (alignment holds)", () => {
  process.stdout.columns = 60;
  const lines = renderTable(
    [
      { id: "a1", title: "short" },
      { id: "longer-id", title: "a much longer title that fills the row" },
    ],
    COLS,
  );
  const widths = new Set(lines.map(visible));
  expect(widths.size).toBe(1);
  // header + 3 borders (top, mid, bottom) + 2 data rows
  expect(lines.length).toBe(6);
});

test("flex column truncates to fit a narrow terminal", () => {
  process.stdout.columns = 30;
  const lines = renderTable([{ id: "x", title: "x".repeat(200) }], COLS);
  for (const line of lines) expect(visible(line)).toBeLessThanOrEqual(30);
  expect(lines.some((l) => l.includes("…"))).toBe(true);
});

test("painted cells do not break alignment", () => {
  process.stdout.columns = 50;
  const ESC = String.fromCharCode(27);
  const green = (c: string): string => `${ESC}[32m${c}${ESC}[0m`;
  const lines = renderTable(
    [
      { id: "a", title: "one" },
      { id: "b", title: "two" },
    ],
    [
      { header: "ID", get: (r: Row) => r.id, paint: (c) => green(c) },
      { header: "TITLE", get: (r: Row) => r.title, flex: true as const },
    ],
  );
  const widths = new Set(lines.map(visible));
  expect(widths.size).toBe(1);
});
