import { helpText } from "@app/cli/help.ts";
import { COPY } from "@app/config/brand.ts";
import { WelcomePanel } from "@app/presentation/WelcomePanel.tsx";
import type { SkillInfo, ToolInfo } from "@app/presentation/useBootstrap.ts";
import type { ChatPort } from "@features/chat/domain/chatPort.ts";
import type { TranscriptEntry } from "@features/chat/domain/transcript.ts";
import { AssistantText } from "@features/chat/presentation/components/AssistantText.tsx";
import { MessageInput } from "@features/chat/presentation/components/MessageInput.tsx";
import { StatusLine } from "@features/chat/presentation/components/StatusLine.tsx";
import { TranscriptItem } from "@features/chat/presentation/components/TranscriptItem.tsx";
import { runChatCommand } from "@features/chat/presentation/runChatCommand.ts";
import { useChat } from "@features/chat/presentation/useChat.ts";
import { providerKeyDiagnostics } from "@features/doctor/diagnostics.ts";
import { BrandHeader } from "@shared/ui/brand/BrandHeader.tsx";
import { palette } from "@shared/ui/tokens/theme.ts";
import { useTerminalSize } from "@shared/ui/useTerminalSize.ts";
// The chat surface: committed transcript via <Static> (prints once, uses the
// terminal's native scrollback), with a live region below for in-flight
// streaming text, the status line, and the input. Owns keyboard input.
import { Box, Static, Text, useApp, useStdin } from "ink";
import { useEffect, useRef, useState } from "react";

interface ChatViewProps {
  readonly port: ChatPort;
  readonly sessionId: string;
  readonly model: string;
  readonly cwd: string;
  readonly home: string;
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

// Keep only the last `n` lines — bounds the live streaming preview's height so
// the redrawn-in-place region never overflows the viewport into scrollback.
function tailLines(text: string, n: number): string {
  const lines = text.split("\n");
  return lines.length <= n ? text : lines.slice(-n).join("\n");
}

export function ChatView({
  port,
  sessionId,
  model,
  cwd,
  home,
  skills,
  tools,
  commandGroups,
}: ChatViewProps) {
  const { exit } = useApp();
  const { isRawModeSupported } = useStdin();
  const { state, sendPrompt, interrupt, respond, note, reset } = useChat(port, sessionId);

  // Prompts typed while a turn is busy are queued (not dropped) and flushed
  // FIFO once it goes idle — so the user can keep typing mid-thinking.
  const queue = useRef<string[]>([]);
  useEffect(() => {
    if (state.phase === "idle" && queue.current.length > 0) {
      const next = queue.current.shift();
      if (next) sendPrompt(next);
    }
  }, [state.phase, sendPrompt]);

  const handleSubmit = (text: string) => {
    const trimmed = text.trim();
    // Slash commands and `regent …` typed in chat run as commands, never sent
    // to the model. `/<cmd>` and `regent <cmd>` route to the same handler.
    if (trimmed.startsWith("/")) return runCommand(trimmed.slice(1));
    const regent = /^regent\s+(.+)/i.exec(trimmed);
    if (regent) return runCommand(regent[1] ?? "");
    if (state.phase === "approving") {
      // Anything that isn't a plain yes travels as feedback: the deny-reason
      // for a tool gate, or the free-text answer to an ask_user question.
      const yes = isAffirmative(text);
      return respond(yes, yes ? undefined : trimmed);
    }
    if (state.phase === "busy") {
      queue.current.push(trimmed);
      return note(`⏳ queued (${queue.current.length}) — sends when the current turn finishes`);
    }
    sendPrompt(text);
  };

  // Chat-native commands are handled locally; every other command + subcommand
  // runs through the real CLI (runChatCommand) so the chat mirrors the shell.
  const runCommand = (line: string) => {
    const cmd = (line.trim().split(/\s+/)[0] ?? "").toLowerCase();
    switch (cmd) {
      case "quit":
      case "exit":
        return exit();
      case "help":
        return note(helpText());
      case "doctor":
        return note(providerKeyDiagnostics(home));
      case "new":
      case "clear":
        return reset();
      case "stop":
        return state.phase === "busy" ? interrupt() : note("nothing is running");
      case "approve":
        return state.phase === "approving" ? respond(true) : note("nothing to approve");
      case "deny":
        return state.phase === "approving" ? respond(false) : note("nothing to deny");
      case "":
        return note("type a command — /help for the list");
      default:
        note(`⚙ ${line.trim()}`);
        runChatCommand(home, line, note);
        return;
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
  const { columns, rows } = useTerminalSize();
  const rule = "─".repeat(Math.max(1, columns - 2));

  // Live streaming preview: only the last few lines. The live region is redrawn
  // in place by Ink; if it ever grows taller than the viewport, Ink can't erase
  // it and spills duplicates into scrollback (the doubled long reply). Bounding
  // it keeps it small — the full reply renders framed once it commits to <Static>.
  const previewMax = Math.max(3, Math.min(10, rows - 12));
  const streamingPreview = tailLines(state.streaming, previewMax);

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
            <Box
              key={`e${item.id}`}
              paddingX={1}
              // Breathing room after each user message + AI reply (turns), while
              // tool/note lines stay compact.
              marginBottom={item.kind === "user" || item.kind === "assistant" ? 1 : 0}
            >
              <TranscriptItem entry={item} />
            </Box>
          )
        }
      </Static>

      <Box flexDirection="column" paddingX={1} marginTop={1}>
        {state.streamingActive && state.streaming.length > 0 ? (
          <Box flexDirection="column">
            <Text bold color={palette.gold}>
              ✦ Regent
            </Text>
            <AssistantText text={streamingPreview} />
          </Box>
        ) : null}
        <StatusLine
          phase={state.phase}
          model={state.model || model}
          contextTokens={state.contextTokens}
          maxContextTokens={state.maxContextTokens}
        />
        <Text color={palette.tealDim}>{rule}</Text>
        <MessageInput
          placeholder={
            state.phase === "approving"
              ? COPY.approvePrompt
              : state.phase === "busy"
                ? COPY.queuePlaceholder
                : COPY.inputPlaceholder
          }
          isActive={Boolean(isRawModeSupported)}
          onSubmit={handleSubmit}
          onCtrlC={handleCtrlC}
        />
        <Text color={palette.tealDim}>{rule}</Text>
        {exitHint ? <Text color={palette.grey}>press Ctrl-C again to exit</Text> : null}
      </Box>
    </Box>
  );
}
