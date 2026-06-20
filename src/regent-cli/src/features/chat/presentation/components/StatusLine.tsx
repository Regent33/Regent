import { COPY } from "@app/config/brand.ts";
import type { ChatPhase } from "@features/chat/domain/transcript.ts";
import { Spinner } from "@shared/ui/components/Spinner.tsx";
import { palette } from "@shared/ui/tokens/theme.ts";
// The status line below the transcript: a spinner while the turn runs, an
// approval prompt, or the idle hint.
import { Box, Text } from "ink";

export function StatusLine({ phase }: { readonly phase: ChatPhase }) {
  if (phase === "approving") {
    return <Text color={palette.teal}> {COPY.awaitingApproval}</Text>;
  }
  if (phase === "busy") {
    return (
      <Box>
        <Spinner />
        <Text color={palette.grey}> {COPY.thinking}</Text>
      </Box>
    );
  }
  return <Text color={palette.grey}> {COPY.idleHint}</Text>;
}
