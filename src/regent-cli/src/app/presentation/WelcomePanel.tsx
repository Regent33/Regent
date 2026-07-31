import { BRAND } from "@app/config/brand.ts";
import type { SkillInfo, ToolInfo } from "@app/presentation/useBootstrap.ts";
import { KING_ART } from "@shared/ui/brand/kingArt.generated.ts";
import { PixelArt } from "@shared/ui/brand/PixelArt.tsx";
import { Panel } from "@shared/ui/components/Panel.tsx";
import { palette } from "@shared/ui/tokens/theme.ts";
// The bordered welcome panel: categorised Skills / Tools / Commands on the
// left (grouped by category), and the kneeling-king mark on the
// right with the model, working directory, and session id centred beneath it.
import { Box, Text } from "ink";

interface WelcomePanelProps {
  readonly model: string;
  readonly cwd: string;
  readonly sessionId: string;
  readonly skills: readonly SkillInfo[];
  readonly tools: readonly ToolInfo[];
  readonly commandGroups: Record<string, readonly string[]>;
}

export function WelcomePanel({
  model,
  cwd,
  sessionId,
  skills,
  tools,
  commandGroups,
}: WelcomePanelProps) {
  // Sized once at launch width (minus the greeting's paddingX), matching the
  // input frame. The greeting commits to native scrollback, so — per the resize
  // model — it intentionally does not reflow afterwards.
  const width = (process.stdout.columns ?? 80) - 2;
  // The art is a rectangle; measure the WIDEST row, not row 0, or a ragged
  // regeneration would silently under-reserve and wrap the mark.
  const kingWidth = KING_ART.reduce((w, row) => Math.max(w, row.length), 0) || 30;
  const lineBudget = leftColumnBudget(width, kingWidth);

  const skillGroups = groupBy(skills, (s) => s.tags[0] ?? "general");
  const toolGroups = groupBy(tools, (t) => t.toolset);

  return (
    <Panel title={`${BRAND.name} v${BRAND.version}`} width={width}>
      {/* Two columns spanning the panel: text hugs the left border, the king
          mark hugs the right — so there's no dead space to the right of the art
          (the earlier flex-start dumped all slack there). The gap sits between
          the columns instead. */}
      <Box marginTop={1} justifyContent="space-between" alignItems="flex-start">
        {/* Left: categorised Skills, Tools, Commands. */}
        <Box flexDirection="column" flexShrink={1} marginRight={4}>
          <CategorySection
            heading="Skills"
            groups={skillGroups}
            empty="none yet — they grow as we work together"
            maxChars={lineBudget}
          />
          <CategorySection
            heading="Tools"
            groups={toolGroups}
            empty="none enabled"
            maxChars={lineBudget}
          />
          <CategorySection heading="Commands" groups={commandGroups} maxChars={lineBudget} />
          <Text color={palette.grey}>
            tip: run any command in chat with / — e.g. /status, /soul
          </Text>
        </Box>
        {/* Right: the king mark, with model / cwd / session centred beneath it.
            flexShrink=0 + explicit width keeps the art's exact shape no matter
            how tall the text column grows. */}
        <Box flexDirection="column" flexShrink={0} width={kingWidth} alignItems="center">
          <PixelArt rows={KING_ART} />
          {/* Every line here is BOTH truncated and wrap-disabled. The model was
              neither: a 40-character id like
              "nvidia/nvidia/nemotron-3-ultra-550b-a55b" wrapped onto a second
              line inside a 30-wide column, growing the art's column and
              breaking the panel's balance. `truncate` keeps the TAIL, which is
              the identifying half of both a model id and a path. */}
          <Box marginTop={1} flexDirection="column" alignItems="center">
            <Text bold wrap="truncate" color={palette.white}>
              {truncate(model, kingWidth)}
            </Text>
            <Text wrap="truncate" color={palette.grey}>
              {truncate(cwd, kingWidth)}
            </Text>
            <Text wrap="truncate" color={palette.tealDim}>
              session {truncate(sessionId, kingWidth - 8)}
            </Text>
          </Box>
        </Box>
      </Box>
    </Panel>
  );
}

// Columns the left text column really has, derived from the layout rather than
// guessed. Panel draws a border (2) and pads by 1 each side (2), and the text
// column keeps marginRight={4} clear of the right-pinned art:
//
//   panel width │ border+padding │ left text … gap 4 … │ king mark │
//
// The old value was `width - kingWidth - 16` — 8 columns tighter than the real
// space, which read as safe but was not, because it budgeted only the ITEMS on
// a line while the line also prints "<category>: " in front of them and " …"
// behind. A category name longer than six characters therefore overran the
// column and Ink wrapped it, shoving the art out of shape. `maxChars` is now
// the budget for the WHOLE rendered line, and this is the honest figure.
export function leftColumnBudget(width: number, kingWidth: number): number {
  const PANEL_CHROME = 4; // border (2) + paddingX (1 each side)
  const COLUMN_GAP = 4; // marginRight on the text column
  return Math.max(24, width - PANEL_CHROME - COLUMN_GAP - kingWidth);
}

/**
 * Fit `category: a, b, c …` into `maxChars` columns TOTAL, including the label
 * and the ellipsis. Returns the two pieces so the caller can colour them
 * separately; `label.length + body.length` is never more than `maxChars`.
 *
 * A single item wider than the whole budget is truncated rather than emitted
 * intact — one 90-character tool name used to blow the line open on its own,
 * because the loop always admitted the first item unconditionally.
 */
export function fitCategory(
  category: string,
  items: readonly string[],
  maxChars: number,
): { label: string; body: string } {
  const label = `${category}: `;
  const ELLIPSIS = " …";
  // Room for the items themselves, keeping space for the ellipsis in case not
  // everything fits. Never negative, so a tiny terminal degrades rather than throws.
  const room = Math.max(0, maxChars - label.length);
  const roomIfTruncated = Math.max(0, room - ELLIPSIS.length);

  const acc: string[] = [];
  for (const item of items.slice(0, MAX_ITEMS)) {
    const next = acc.length === 0 ? item : `${acc.join(", ")}, ${item}`;
    // Everything so far plus this one still fits with room for a trailing "…"
    // if more follow. Checked against the composed string, not a running total,
    // so separators cannot be miscounted.
    if (next.length > roomIfTruncated && acc.length > 0) break;
    acc.push(item);
  }

  if (acc.length === 0) {
    // Nothing fits — show as much of the first item as the column allows.
    const first = items[0] ?? "";
    return { label, body: first.slice(0, roomIfTruncated) + (first ? ELLIPSIS : "") };
  }
  const shown = acc.join(", ");
  const complete = acc.length === items.length;
  if (complete && shown.length <= room) return { label, body: shown };
  return { label, body: shown.slice(0, roomIfTruncated) + ELLIPSIS };
}

// Bucket items by category, preserving each category's insertion order.
function groupBy<T extends { name: string }>(
  items: readonly T[],
  category: (item: T) => string,
): Record<string, string[]> {
  const out: Record<string, string[]> = {};
  for (const item of items) {
    const key = category(item);
    out[key] ??= [];
    out[key].push(item.name);
  }
  return out;
}

// Cap rows + items so the left column stays compact and never dwarfs the king
// mark on the right; overflow collapses to a "…" so the section reads at a
// glance. MAX_ROWS bounds height; MAX_ITEMS bounds each line's width.
const MAX_ROWS = 6;
const MAX_ITEMS = 12;

function CategorySection({
  heading,
  groups,
  empty,
  maxChars,
}: {
  heading: string;
  groups: Record<string, readonly string[]>;
  empty?: string;
  maxChars: number;
}) {
  const categories = Object.keys(groups).sort();
  const shown = categories.slice(0, MAX_ROWS);
  const hiddenRows = categories.length - shown.length;
  return (
    <Box flexDirection="column" marginBottom={1}>
      <Text bold color={palette.teal}>
        {heading}
      </Text>
      {categories.length === 0 ? (
        <Text color={palette.grey}>{empty ?? "—"}</Text>
      ) : (
        <>
          {shown.map((c) => (
            <CategoryLine key={c} category={c} items={groups[c] ?? []} maxChars={maxChars} />
          ))}
          {hiddenRows > 0 && <Text color={palette.grey}>… +{hiddenRows} more</Text>}
        </>
      )}
    </Box>
  );
}

// `category: a, b, c …`, fitted to `maxChars` by `fitCategory` above. The
// wrapping is disabled explicitly: Ink would otherwise reflow an over-long line
// into the gap beside the art, which is the failure this is here to prevent —
// and if the fit is ever wrong, a clipped line is a far better symptom than a
// mangled one.
function CategoryLine({
  category,
  items,
  maxChars,
}: {
  category: string;
  items: readonly string[];
  maxChars: number;
}) {
  const { label, body } = fitCategory(category, items, maxChars);
  return (
    <Text wrap="truncate">
      <Text color={palette.tealDim}>{label}</Text>
      <Text color={palette.grey}>{body}</Text>
    </Text>
  );
}

export function truncate(s: string, max: number): string {
  return s.length > max ? `…${s.slice(s.length - max + 1)}` : s;
}
