import { BRAND } from "@app/config/brand.ts";
import type { SkillInfo, ToolInfo } from "@app/presentation/useBootstrap.ts";
import { PixelArt } from "@shared/ui/brand/PixelArt.tsx";
import { KING_ART } from "@shared/ui/brand/kingArt.generated.ts";
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
  const kingWidth = KING_ART[0]?.length ?? 30;
  // Per-line item budget for the left column: fill the space to the left of the
  // (right-pinned) king mark before collapsing to "…". Responsive, so a wide
  // terminal shows more skills/tools per line; a wider left column only fills the
  // middle gap — it can't push the art, which is anchored to the right border.
  const lineBudget = Math.max(44, width - kingWidth - 16);

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
          <Box marginTop={1} flexDirection="column" alignItems="center">
            <Text bold color={palette.white}>
              {model}
            </Text>
            <Text color={palette.grey}>{truncate(cwd, kingWidth)}</Text>
            <Text color={palette.tealDim}>session {truncate(sessionId, kingWidth - 8)}</Text>
          </Box>
        </Box>
      </Box>
    </Panel>
  );
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

// `category: a, b, c …` — capped by BOTH item count and the (responsive)
// `maxChars` width budget so one long category can't overflow the left column.
// Overflow collapses to "…".
function CategoryLine({
  category,
  items,
  maxChars,
}: {
  category: string;
  items: readonly string[];
  maxChars: number;
}) {
  const acc: string[] = [];
  let len = 0;
  for (const item of items.slice(0, MAX_ITEMS)) {
    // Keep at least one item; stop once adding the next would blow the width.
    if (acc.length > 0 && len + item.length + 2 > maxChars) break;
    acc.push(item);
    len += item.length + 2;
  }
  const shown = acc.join(", ");
  const more = acc.length < items.length ? " …" : "";
  return (
    <Text>
      <Text color={palette.tealDim}>{category}: </Text>
      <Text color={palette.grey}>
        {shown}
        {more}
      </Text>
    </Text>
  );
}

function truncate(s: string, max: number): string {
  return s.length > max ? `…${s.slice(s.length - max + 1)}` : s;
}
