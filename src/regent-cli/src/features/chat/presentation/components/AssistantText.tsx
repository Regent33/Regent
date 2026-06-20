import { splitThinking } from "@features/chat/domain/thinking.ts";
import { MarkdownText } from "@features/chat/presentation/components/Markdown.tsx";
import { palette } from "@shared/ui/tokens/theme.ts";
// Renders assistant output: any <think>…</think> reasoning shown dim/italic
// under a "✻ Thinking" header (Claude-Code style); the answer through the
// markdown renderer (proper tables, etc.).
import { Box, Text } from "ink";

export function AssistantText({ text }: { readonly text: string }) {
  const { thinking, answer } = splitThinking(text);
  if (!thinking) return <MarkdownText text={answer || text} />;
  return (
    <Box flexDirection="column">
      <Text italic color={palette.tealDim}>
        ✻ Thinking
      </Text>
      <Text italic color={palette.grey}>
        {thinking}
      </Text>
      {answer ? (
        <Box marginTop={1}>
          <MarkdownText text={answer} />
        </Box>
      ) : null}
    </Box>
  );
}
