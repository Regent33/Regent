import { palette } from "@shared/ui/tokens/theme.ts";
// Lightweight markdown rendering for assistant output. Focused on GitHub-style
// tables (aligned columns + a ruled header); everything else passes through as
// plain text. Keeps raw `| a | b |` from leaking into the terminal.
import { Box, Text } from "ink";

const isRow = (l: string): boolean => /^\s*\|.*\|\s*$/.test(l);
// A separator row: pipes, dashes, colons, spaces — and at least one dash.
const isSep = (l: string): boolean => /^\s*\|?[\s:|-]+\|?\s*$/.test(l) && l.includes("-");

function cells(row: string): string[] {
  return row
    .trim()
    .replace(/^\||\|$/g, "")
    .split("|")
    .map((c) => c.trim());
}

interface Block {
  readonly key: string;
  readonly kind: "text" | "table";
  readonly lines: string[];
}

function parseBlocks(text: string): Block[] {
  const lines = text.split("\n");
  const blocks: Block[] = [];
  let i = 0;
  const startsTable = (n: number): boolean =>
    isRow(lines[n] ?? "") && n + 1 < lines.length && isSep(lines[n + 1] ?? "");
  while (i < lines.length) {
    if (startsTable(i)) {
      const tbl = [lines[i] ?? "", lines[i + 1] ?? ""];
      i += 2;
      while (i < lines.length && isRow(lines[i] ?? "")) {
        tbl.push(lines[i] ?? "");
        i += 1;
      }
      blocks.push({ key: `t${blocks.length}`, kind: "table", lines: tbl });
      continue;
    }
    const txt: string[] = [];
    while (i < lines.length && !startsTable(i)) {
      txt.push(lines[i] ?? "");
      i += 1;
    }
    blocks.push({ key: `x${blocks.length}`, kind: "text", lines: txt });
  }
  return blocks;
}

function Table({ lines }: { readonly lines: string[] }) {
  const header = cells(lines[0] ?? "");
  const rows = lines.slice(2).map(cells); // skip header + separator
  const cols = header.length;
  const widths = Array.from({ length: cols }, (_, c) =>
    Math.max(header[c]?.length ?? 0, ...rows.map((r) => r[c]?.length ?? 0)),
  );
  const fmt = (cs: string[]): string => widths.map((w, c) => (cs[c] ?? "").padEnd(w)).join("  ");
  const rule = widths.map((w) => "─".repeat(w)).join("  ");
  return (
    <Box flexDirection="column" marginY={1}>
      <Text bold color={palette.teal}>
        {fmt(header)}
      </Text>
      <Text color={palette.tealDim}>{rule}</Text>
      {rows.map((r, i) => (
        <Text key={`${r.join("|")}-${i}`} color={palette.white}>
          {fmt(r)}
        </Text>
      ))}
    </Box>
  );
}

export function MarkdownText({ text }: { readonly text: string }) {
  const blocks = parseBlocks(text);
  return (
    <Box flexDirection="column">
      {blocks.map((b) =>
        b.kind === "table" ? (
          <Table key={b.key} lines={b.lines} />
        ) : (
          <Text key={b.key} color={palette.white}>
            {b.lines.join("\n")}
          </Text>
        ),
      )}
    </Box>
  );
}
