import { BRAND } from "@app/config/brand.ts";
import type { SkillInfo, ToolInfo } from "@app/presentation/useBootstrap.ts";
import { PixelArt } from "@shared/ui/brand/PixelArt.tsx";
import { KING_ART } from "@shared/ui/brand/kingArt.generated.ts";
import { Panel } from "@shared/ui/components/Panel.tsx";
import { palette } from "@shared/ui/tokens/theme.ts";
// The bordered welcome panel: categorised Skills / Tools / Commands on the
// left (grouped by category, Hermes-style), and the kneeling-king mark on the
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

  const skillGroups = groupBy(skills, (s) => s.tags[0] ?? "general");
  const toolGroups = groupBy(tools, (t) => t.toolset);

  return (
    <Panel title={`${BRAND.name} v${BRAND.version}`} width={width}>
      <Box marginTop={1} justifyContent="center" alignItems="flex-start">
        {/* Left: categorised Skills, Tools, Commands. */}
        <Box flexDirection="column" flexShrink={1} marginRight={6}>
          <CategorySection
            heading="Skills"
            groups={skillGroups}
            empty="none yet — they grow as we work together"
          />
          <CategorySection heading="Tools" groups={toolGroups} empty="none enabled" />
          <CategorySection heading="Commands" groups={commandGroups} />
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
    (out[key] ??= []).push(item.name);
  }
  return out;
}

function CategorySection({
  heading,
  groups,
  empty,
}: {
  heading: string;
  groups: Record<string, readonly string[]>;
  empty?: string;
}) {
  const categories = Object.keys(groups).sort();
  return (
    <Box flexDirection="column" marginBottom={1}>
      <Text bold color={palette.teal}>
        {heading}
      </Text>
      {categories.length === 0 ? (
        <Text color={palette.grey}>{empty ?? "—"}</Text>
      ) : (
        categories.map((c) => <CategoryLine key={c} category={c} items={groups[c] ?? []} />)
      )}
    </Box>
  );
}

// `category: a, b, c (+N more)` — capped so a busy category stays one line.
function CategoryLine({ category, items }: { category: string; items: readonly string[] }) {
  const MAX = 5;
  const shown = items.slice(0, MAX).join(", ");
  const more = items.length > MAX ? ` (+${items.length - MAX} more)` : "";
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
