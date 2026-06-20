import { spaceEmoji, splitThinking } from "@features/chat/domain/thinking.ts";
import { MarkdownText } from "@features/chat/presentation/components/Markdown.tsx";
import { palette } from "@shared/ui/tokens/theme.ts";
// Renders assistant output: any <think>…</think> reasoning shown dim/italic
// under a "✻ Thinking" header (Claude-Code style); the answer through the
// markdown renderer (proper tables, etc.).
import { Box, Text } from "ink";

export function AssistantText({ text }: { readonly text: string }) {
  const { thinking, answer } = splitThinking(text);
  // Render the stripped answer — never fall back to raw `text` (that re-leaks a
  // stray `</think>`); nothing to show if it's empty after stripping.
  if (!thinking) return answer ? <MarkdownText text={answer} /> : null;
  return (
    <Box flexDirection="column">
      <Text italic color={palette.tealDim}>
        ✻ Thinking
      </Text>
      <Text italic color={palette.grey}>
        {spaceEmoji(thinking)}
      </Text>
      {answer ? (
        <Box marginTop={1}>
          <MarkdownText text={answer} />
        </Box>
      ) : null}
    </Box>
  );
}
