import { COPY } from "@app/config/brand.ts";
import { CHAT_SLASH, CLI_COMMAND_GROUPS } from "@app/config/commands.ts";
import { useBootstrap } from "@app/presentation/useBootstrap.ts";
import { createRpcChatAdapter } from "@features/chat/data/rpcChatAdapter.ts";
import { ChatView } from "@features/chat/presentation/ChatView.tsx";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { BrandHeader } from "@shared/ui/brand/BrandHeader.tsx";
import { Panel } from "@shared/ui/components/Panel.tsx";
import { Spinner } from "@shared/ui/components/Spinner.tsx";
import { palette } from "@shared/ui/tokens/theme.ts";
// Root shell (thin): a connect→ready/error state machine. On `connecting`/
// `error` it shows the brand header + status; on `ready` it hands off to the
// chat surface, which owns keyboard input from there.
import { Box, Text, useApp, useInput, useStdin } from "ink";
import { useMemo } from "react";

export function App({
  client,
  resumeSessionId,
  home,
}: {
  readonly client: IRpcClient;
  readonly resumeSessionId: string | undefined;
  readonly home: string;
}) {
  const { exit } = useApp();
  const { isRawModeSupported } = useStdin();
  const state = useBootstrap(client, resumeSessionId);

  // The chat surface owns input once ready; before that, q / Esc / Ctrl-C abort.
  useInput(
    (input, key) => {
      if (input === "q" || key.escape || (key.ctrl && input === "c")) exit();
    },
    { isActive: Boolean(isRawModeSupported) && state.phase !== "ready" },
  );

  // Session-scoped chat port, built once the session exists.
  const port = useMemo(
    () => createRpcChatAdapter(client, state.sessionId),
    [client, state.sessionId],
  );

  if (state.phase === "ready") {
    return (
      <ChatView
        port={port}
        sessionId={state.sessionId}
        model={state.model}
        cwd={process.cwd()}
        home={home}
        skills={state.skills}
        tools={state.tools}
        commandGroups={{ ...CLI_COMMAND_GROUPS, chat: CHAT_SLASH }}
      />
    );
  }

  return (
    <Box flexDirection="column" paddingX={1}>
      <BrandHeader />
      {state.phase === "connecting" && (
        <Box>
          <Spinner />
          <Text color={palette.grey}> {COPY.connecting}</Text>
        </Box>
      )}
      {state.phase === "error" && (
        <Panel
          title={COPY.errorTitle}
          width={Math.max(state.error.length, COPY.errorHint.length, COPY.errorTitle.length) + 6}
        >
          <Text color="red">{state.error}</Text>
          <Box marginTop={1}>
            <Text color={palette.grey}>{COPY.errorHint}</Text>
          </Box>
        </Panel>
      )}
    </Box>
  );
}
