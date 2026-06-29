import { COPY } from "@app/config/brand.ts";
import type { ChatPhase } from "@features/chat/domain/transcript.ts";
import { Spinner } from "@shared/ui/components/Spinner.tsx";
import { palette } from "@shared/ui/tokens/theme.ts";
// The status line below the transcript — a Hermes-style meta bar (model ·
// context-fill bar · elapsed) plus the live state (spinner / approval / idle).
import { Box, Text } from "ink";
import { useEffect, useRef, useState } from "react";

interface StatusLineProps {
  readonly phase: ChatPhase;
  readonly model: string;
  readonly contextTokens: number;
  readonly maxContextTokens: number;
}

const BAR_WIDTH = 12;

/** Compact token count: 16100 → "16.1K", 524300 → "524.3K". */
function fmtK(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(1)}K` : String(n);
}

/** Seconds since the turn went busy; freezes at the last value once idle. */
function useElapsed(active: boolean): number {
  const [secs, setSecs] = useState(0);
  const start = useRef<number | null>(null);
  useEffect(() => {
    if (!active) return;
    start.current = Date.now();
    setSecs(0);
    const id = setInterval(() => {
      if (start.current) setSecs(Math.floor((Date.now() - start.current) / 1000));
    }, 1000);
    return () => clearInterval(id);
  }, [active]);
  return secs;
}

export function StatusLine({ phase, model, contextTokens, maxContextTokens }: StatusLineProps) {
  const elapsed = useElapsed(phase === "busy");
  const pct =
    maxContextTokens > 0 ? Math.min(100, Math.round((contextTokens / maxContextTokens) * 100)) : 0;
  const filled = Math.round((pct / 100) * BAR_WIDTH);

  const meta = (
    <Text>
      <Text bold color={palette.gold}>
        ✦ {model || "regent"}
      </Text>
      {maxContextTokens > 0 ? (
        <Text>
          <Text color={palette.grey}>
            {"  "}
            {fmtK(contextTokens)}/{fmtK(maxContextTokens)}{" "}
          </Text>
          <Text color={palette.teal}>{"█".repeat(filled)}</Text>
          <Text color={palette.grey}>
            {"░".repeat(BAR_WIDTH - filled)} {pct}%
          </Text>
        </Text>
      ) : null}
      {elapsed > 0 ? (
        <Text color={palette.grey}>
          {"  "}
          {elapsed}s
        </Text>
      ) : null}
    </Text>
  );

  if (phase === "approving") {
    return (
      <Box flexDirection="column">
        {meta}
        <Text color={palette.teal}> {COPY.awaitingApproval}</Text>
      </Box>
    );
  }
  if (phase === "busy") {
    return (
      <Box>
        <Spinner />
        <Text color={palette.grey}> {COPY.thinking} </Text>
        {meta}
      </Box>
    );
  }
  return (
    <Box>
      {meta}
      <Text color={palette.grey}>
        {"  "}· {COPY.idleHint}
      </Text>
    </Box>
  );
}
