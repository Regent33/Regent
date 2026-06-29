import { COPY } from "@app/config/brand.ts";
import { splitThinking } from "@features/chat/domain/thinking.ts";
import type { TranscriptEntry } from "@features/chat/domain/transcript.ts";
import { AssistantFrame } from "@features/chat/presentation/components/AssistantFrame.tsx";
import { AssistantText } from "@features/chat/presentation/components/AssistantText.tsx";
import { palette } from "@shared/ui/tokens/theme.ts";
// Renders one committed transcript entry. Pure presentation — palette + copy
// only; the entry shape comes from the domain reducer.
import { Box, Text } from "ink";

export function TranscriptItem({ entry }: { readonly entry: TranscriptEntry }) {
  switch (entry.kind) {
    case "user":
      return (
        <Text>
          <Text color={palette.teal}>❯ </Text>
          <Text color={palette.white}>{entry.text}</Text>
        </Text>
      );
    case "assistant": {
      // Decide emptiness from the *rendered* content, not the raw text: a
      // mid-turn message that's only a stray/partial <think> tag strips to
      // nothing and must not draw an empty box. Frame only the final reply
      // (Hermes shows one box per turn); mid-turn preambles render plain.
      const { thinking, answer } = splitThinking(entry.text);
      if (!answer && !thinking) return null;
      const body = <AssistantText text={entry.text} />;
      // Final reply → full box. Mid-turn preamble ("on it, searching…") → a dim
      // labelled line so Regent's acknowledgment is visible without a box.
      return entry.final ? (
        <AssistantFrame>{body}</AssistantFrame>
      ) : (
        <Box flexDirection="column" paddingLeft={1}>
          <Text color={palette.tealDim}>✦ Regent</Text>
          {body}
        </Box>
      );
    }
    case "tool":
      return <Text color={palette.tealDim}> {COPY.toolRunning(entry.tool)}</Text>;
    case "toolError":
      return <Text color={palette.grey}> {COPY.toolSnag(entry.tool)}</Text>;
    case "approvalAsk":
      return (
        <Box flexDirection="column">
          <Text color={palette.teal}>{COPY.approvalWarn(entry.tool)}</Text>
          <Text color={palette.white}> {entry.action}</Text>
        </Box>
      );
    case "approvalResolved":
      return <Text color={palette.grey}> {entry.approved ? COPY.approved : COPY.denied}</Text>;
    case "outbound":
      return (
        <Text>
          <Text color={palette.teal}>{COPY.delivered(entry.target)}</Text>
          <Text color={palette.white}>: {entry.text}</Text>
        </Text>
      );
    case "note":
      return <Text color={palette.grey}>{entry.text}</Text>;
  }
}
