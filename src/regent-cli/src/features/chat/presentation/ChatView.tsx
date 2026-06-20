import { helpText } from "@app/cli/help.ts";
import { COPY } from "@app/config/brand.ts";
import { WelcomePanel } from "@app/presentation/WelcomePanel.tsx";
import type { SkillInfo, ToolInfo } from "@app/presentation/useBootstrap.ts";
import type { ChatPort } from "@features/chat/domain/chatPort.ts";
import type { TranscriptEntry } from "@features/chat/domain/transcript.ts";
import { MessageInput } from "@features/chat/presentation/components/MessageInput.tsx";
import { StatusLine } from "@features/chat/presentation/components/StatusLine.tsx";
import { TranscriptItem } from "@features/chat/presentation/components/TranscriptItem.tsx";
import { useChat } from "@features/chat/presentation/useChat.ts";
import { BrandHeader } from "@shared/ui/brand/BrandHeader.tsx";
import { palette } from "@shared/ui/tokens/theme.ts";
import { useTerminalSize } from "@shared/ui/useTerminalSize.ts";
// The chat surface: committed transcript via <Static> (prints once, uses the
// terminal's native scrollback), with a live region below for in-flight
// streaming text, the status line, and the input. Owns keyboard input.
import { Box, Static, Text, useApp, useStdin } from "ink";
import { useRef, useState } from "react";

interface ChatViewProps {
  readonly port: ChatPort;
  readonly sessionId: string;
  readonly model: string;
  readonly cwd: string;
  readonly skills: readonly SkillInfo[];
  readonly tools: readonly ToolInfo[];
  readonly commandGroups: Record<string, readonly string[]>;
}

// The greeting is the first <Static> item, so it prints once above the chat.
type StaticItem = { kind: "greeting" } | TranscriptEntry;

const isAffirmative = (text: string): boolean => {
  const t = text.toLowerCase();
  return t === "y" || t === "yes";
};

export function ChatView({
  port,
  sessionId,
  model,
  cwd,
  skills,
  tools,
  commandGroups,
}: ChatViewProps) {
  const { exit } = useApp();
  const { isRawModeSupported } = useStdin();
  const { state, sendPrompt, interrupt, respond, note, reset } = useChat(port, sessionId);

  const handleSubmit = (text: string) => {
    const trimmed = text.trim();
    // Slash commands are handled locally and NEVER sent to the model.
    if (trimmed.startsWith("/")) return runSlash(trimmed.slice(1).split(/\s+/)[0] ?? "");
    if (state.phase === "approving") return respond(isAffirmative(text));
    sendPrompt(text);
  };

  const runSlash = (cmd: string) => {
    switch (cmd.toLowerCase()) {
      case "quit":
      case "exit":
        return exit();
      case "help":
        return note(helpText());
      case "new":
      case "clear":
        return reset();
      case "stop":
        return state.phase === "busy" ? interrupt() : note("nothing is running");
      case "approve":
        return state.phase === "approving" ? respond(true) : note("nothing to approve");
      case "deny":
        return state.phase === "approving" ? respond(false) : note("nothing to deny");
      default:
        return note(`unknown command: /${cmd} — type /help`);
    }
  };

  // Ctrl-C interrupts a running turn; a second Ctrl-C within 1.5s exits, so a
  // single press never quits by accident.
  const lastCtrlC = useRef(0);
  const hintTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [exitHint, setExitHint] = useState(false);

  const handleCtrlC = () => {
    const now = Date.now();
    if (now - lastCtrlC.current < 1500) {
      exit();
      return;
    }
    lastCtrlC.current = now;
    if (state.phase !== "idle") interrupt();
    setExitHint(true);
    if (hintTimer.current) clearTimeout(hintTimer.current);
    hintTimer.current = setTimeout(() => setExitHint(false), 1500);
  };

  const items: StaticItem[] = [{ kind: "greeting" }, ...state.entries];

  // Full-width rule that frames the input (Claude-style), reactive to resize.
  const { columns } = useTerminalSize();
  const rule = "─".repeat(Math.max(1, columns - 2));

  return (
    <Box flexDirection="column">
      <Static items={items}>
        {(item) =>
          item.kind === "greeting" ? (
            <Box key="greeting" flexDirection="column" paddingX={1}>
              <BrandHeader />
              <WelcomePanel
                model={model}
                cwd={cwd}
                sessionId={sessionId}
                skills={skills}
                tools={tools}
                commandGroups={commandGroups}
              />
              <Box marginTop={1}>
                <Text bold color={palette.white}>
                  {COPY.welcome}
                </Text>
              </Box>
            </Box>
          ) : (
            <Box key={`e${item.id}`} paddingX={1}>
              <TranscriptItem entry={item} />
            </Box>
          )
        }
      </Static>

      <Box flexDirection="column" paddingX={1} marginTop={1}>
        {state.streamingActive && state.streaming.length > 0 ? (
          <Text color={palette.white}>{state.streaming}</Text>
        ) : null}
        <StatusLine phase={state.phase} />
        <Text color={palette.tealDim}>{rule}</Text>
        <MessageInput
          placeholder={state.phase === "approving" ? COPY.approvePrompt : COPY.inputPlaceholder}
          isActive={Boolean(isRawModeSupported)}
          acceptInput={state.phase !== "busy"}
          onSubmit={handleSubmit}
          onCtrlC={handleCtrlC}
        />
        <Text color={palette.tealDim}>{rule}</Text>
        {exitHint ? <Text color={palette.grey}>press Ctrl-C again to exit</Text> : null}
      </Box>
    </Box>
  );
}
