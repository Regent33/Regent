import { palette } from "@shared/ui/tokens/theme.ts";
// A controlled single-line input with cursor editing and command history.
// Printable keys insert at the cursor; ←/→ move it; Backspace/Delete edit
// around it; ↑/↓ recall submitted prompts; Enter submits; Ctrl-C delegates.
import { Box, Text, useInput } from "ink";
import { useEffect, useRef, useState } from "react";

interface MessageInputProps {
  readonly placeholder: string;
  readonly isActive: boolean;
  /** When false, Enter is ignored and the typed text is preserved (turn busy). */
  readonly acceptInput: boolean;
  readonly onSubmit: (text: string) => void;
  readonly onCtrlC: () => void;
}

export function MessageInput({
  placeholder,
  isActive,
  acceptInput,
  onSubmit,
  onCtrlC,
}: MessageInputProps) {
  const [value, setValue] = useState("");
  const [pos, setPos] = useState(0);
  const [caretOn, setCaretOn] = useState(true);
  const history = useRef<string[]>([]);
  // -1 = live draft; 0 = newest history entry, increasing = older.
  const [histCursor, setHistCursor] = useState(-1);

  // Blink the caret while active (Ink hides the hardware cursor).
  useEffect(() => {
    if (!isActive) return;
    const id = setInterval(() => setCaretOn((on) => !on), 530);
    return () => clearInterval(id);
  }, [isActive]);

  const set = (text: string, caret = text.length) => {
    setValue(text);
    setPos(Math.max(0, Math.min(caret, text.length)));
  };

  const recall = (delta: number) => {
    const h = history.current;
    if (h.length === 0) return;
    const next = histCursor + delta;
    if (next < -1 || next >= h.length) return;
    setHistCursor(next);
    set(next === -1 ? "" : (h[h.length - 1 - next] ?? ""));
  };

  useInput(
    (input, key) => {
      if (key.ctrl && input === "c") return onCtrlC();
      if (key.return) {
        const text = value.trim();
        if (!text || !acceptInput) return;
        if (history.current.at(-1) !== text) history.current.push(text);
        setHistCursor(-1);
        set("");
        onSubmit(text);
        return;
      }
      if (key.upArrow) return recall(1);
      if (key.downArrow) return recall(-1);
      if (key.leftArrow) return setPos((p) => Math.max(0, p - 1));
      if (key.rightArrow) return setPos((p) => Math.min(value.length, p + 1));
      // Delete the char before the cursor. Terminals disagree on whether the
      // Backspace key reports as `backspace` or `delete`, so treat both that
      // way (otherwise Backspace is a no-op when the cursor is at end-of-line,
      // e.g. right after recalling a history entry).
      if (key.backspace || key.delete) {
        if (pos > 0) set(value.slice(0, pos - 1) + value.slice(pos), pos - 1);
        return;
      }
      // Insert printable input at the cursor; ignore control/meta chords.
      if (input && !key.ctrl && !key.meta && !key.escape) {
        set(value.slice(0, pos) + input + value.slice(pos), pos + input.length);
      }
    },
    { isActive },
  );

  const caretBlock = (ch: string) =>
    caretOn ? (
      <Text color="#000000" backgroundColor={palette.teal}>
        {ch}
      </Text>
    ) : (
      <Text color={palette.white}>{ch}</Text>
    );

  return (
    <Box>
      <Text color={palette.teal}>❯ </Text>
      {value === "" ? (
        <>
          {caretBlock(" ")}
          <Text color={palette.grey}>{placeholder}</Text>
        </>
      ) : (
        <>
          <Text color={palette.white}>{value.slice(0, pos)}</Text>
          {caretBlock(value.slice(pos, pos + 1) || " ")}
          <Text color={palette.white}>{value.slice(pos + 1)}</Text>
        </>
      )}
    </Box>
  );
}
