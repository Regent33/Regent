import { spaceEmoji } from "@features/chat/domain/thinking.ts";
import { palette } from "@shared/ui/tokens/theme.ts";
// Lightweight markdown rendering for assistant output: inline **bold**,
// *italic*, `code`; headings (#…), bullet/numbered lists; and GitHub-style
// tables (aligned + ruled). Keeps raw markup from leaking into the terminal.
import { Box, Text } from "ink";

// ── inline spans ─────────────────────────────────────────────────────────────
interface Span {
  readonly text: string;
  readonly bold?: boolean;
  readonly italic?: boolean;
  readonly code?: boolean;
}

function parseInline(text: string): Span[] {
  const re = /(`[^`]+`|\*\*[^*]+\*\*|__[^_]+__|\*[^*\s][^*]*\*|_[^_\s][^_]*_)/g;
  const spans: Span[] = [];
  let last = 0;
  let m: RegExpExecArray | null = re.exec(text);
  while (m !== null) {
    if (m.index > last) spans.push({ text: text.slice(last, m.index) });
    const t = m[0];
    if (t.startsWith("`")) spans.push({ text: t.slice(1, -1), code: true });
    else if (t.startsWith("**") || t.startsWith("__"))
      spans.push({ text: t.slice(2, -2), bold: true });
    else spans.push({ text: t.slice(1, -1), italic: true });
    last = m.index + t.length;
    m = re.exec(text);
  }
  if (last < text.length) spans.push({ text: text.slice(last) });
  return spans.length > 0 ? spans : [{ text }];
}

const stripInline = (text: string): string =>
  parseInline(text)
    .map((s) => s.text)
    .join("");

function spans(text: string, color: string) {
  return parseInline(text).map((s, i) => (
    <Text
      key={`${i}-${s.text}`}
      bold={Boolean(s.bold)}
      italic={Boolean(s.italic)}
      color={s.code ? palette.teal : color}
    >
      {s.text}
    </Text>
  ));
}

function TextLine({ line: raw }: { readonly line: string }) {
  const line = spaceEmoji(raw);
  const heading = /^(#{1,6})\s+(.*)$/.exec(line);
  if (heading) {
    return (
      <Text bold color={palette.teal}>
        {spans(heading[2] ?? "", palette.teal)}
      </Text>
    );
  }
  const list = /^(\s*)(?:[-*+]|\d+\.)\s+(.*)$/.exec(line);
  if (list) {
    return (
      <Text color={palette.white}>
        {`${list[1] ?? ""}  • `}
        {spans(list[2] ?? "", palette.white)}
      </Text>
    );
  }
  if (line.trim() === "") return <Text> </Text>;
  return <Text color={palette.white}>{spans(line, palette.white)}</Text>;
}

// ── tables ───────────────────────────────────────────────────────────────────
const isRow = (l: string): boolean => /^\s*\|.*\|\s*$/.test(l);
const isSep = (l: string): boolean => /^\s*\|?[\s:|-]+\|?\s*$/.test(l) && l.includes("-");

function cells(row: string): string[] {
  return row
    .trim()
    .replace(/^\||\|$/g, "")
    .split("|")
    .map((c) => stripInline(c.trim()));
}

function Table({ lines }: { readonly lines: string[] }) {
  const header = cells(lines[0] ?? "");
  const rows = lines.slice(2).map(cells);
  const widths = Array.from({ length: header.length }, (_, c) =>
    Math.max(header[c]?.length ?? 0, ...rows.map((r) => r[c]?.length ?? 0)),
  );
  const fmt = (cs: string[]): string => widths.map((w, c) => (cs[c] ?? "").padEnd(w)).join("  ");
  return (
    <Box flexDirection="column" marginY={1}>
      <Text bold color={palette.teal}>
        {fmt(header)}
      </Text>
      <Text color={palette.tealDim}>{widths.map((w) => "─".repeat(w)).join("  ")}</Text>
      {rows.map((r, i) => (
        <Text key={`${r.join("|")}-${i}`} color={palette.white}>
          {fmt(r)}
        </Text>
      ))}
    </Box>
  );
}

// ── blocks ───────────────────────────────────────────────────────────────────
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

export function MarkdownText({ text }: { readonly text: string }) {
  return (
    <Box flexDirection="column">
      {parseBlocks(text).map((b) =>
        b.kind === "table" ? (
          <Table key={b.key} lines={b.lines} />
        ) : (
          <Box key={b.key} flexDirection="column">
            {b.lines.map((l, i) => (
              <TextLine key={`${b.key}-${i}`} line={l} />
            ))}
          </Box>
        ),
      )}
    </Box>
  );
}
