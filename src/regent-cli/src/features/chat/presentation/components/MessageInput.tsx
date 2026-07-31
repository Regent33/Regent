import { matchSlash } from "@app/config/commands.ts";
import {
  applyKey,
  at,
  BINDINGS,
  type Composer,
  EMPTY,
  PASTE_OFF,
  PASTE_ON,
} from "@features/chat/domain/composer.ts";
import { CommandMenu } from "@features/chat/presentation/components/CommandMenu.tsx";
import { palette } from "@shared/ui/tokens/theme.ts";
// A controlled multi-line input. The editing model (cursor, words, lines,
// paste) lives in ../../domain/composer.ts as pure functions; this file is the
// rendering and the wiring — history recall, the `/` picker, and the keys
// overlay. Typing `/` opens the picker: ↑/↓ select, ⇥ complete, ↵ run, esc close.
import { Box, Text, useInput } from "ink";
import { useEffect, useRef, useState } from "react";

interface MessageInputProps {
  readonly placeholder: string;
  readonly isActive: boolean;
  /** Enter always submits; the orchestrator routes by phase (send / queue while
   *  busy / answer an approval). */
  readonly onSubmit: (text: string) => void;
  readonly onCtrlC: () => void;
}

export function MessageInput({ placeholder, isActive, onSubmit, onCtrlC }: MessageInputProps) {
  const [state, setState] = useState<Composer>(EMPTY);
  const { value, pos } = state;
  const history = useRef<string[]>([]);
  // -1 = live draft; 0 = newest history entry, increasing = older.
  const [histCursor, setHistCursor] = useState(-1);
  const [showKeys, setShowKeys] = useState(false);

  // Bracketed paste: the terminal wraps pasted text in markers, so a pasted
  // stack trace is one value instead of the first newline sending it and the
  // rest arriving as separate turns. Turned off again on unmount — leaving a
  // terminal in bracketed-paste mode is rude to whatever runs next.
  useEffect(() => {
    if (!isActive) return;
    process.stdout.write(`${PASTE_ON}`);
    return () => {
      process.stdout.write(`${PASTE_OFF}`);
    };
  }, [isActive]);

  const submit = (text: string) => {
    const trimmed = text.trim();
    if (!trimmed) return;
    if (history.current.at(-1) !== trimmed) history.current.push(trimmed);
    setHistCursor(-1);
    setState(EMPTY);
    onSubmit(trimmed);
  };

  const recall = (delta: number) => {
    const h = history.current;
    if (h.length === 0) return;
    const next = histCursor + delta;
    if (next < -1 || next >= h.length) return;
    setHistCursor(next);
    const text = next === -1 ? "" : (h[h.length - 1 - next] ?? "");
    setState(at(text, text.length));
  };

  // `/` command picker. `matches` is null unless the input is a bare `/prefix`
  // (no space yet); the menu shows while there are matches and Esc hasn't
  // dismissed it. `selected` is clamped so a narrowing filter can't strand it.
  const [sel, setSel] = useState(0);
  const [dismissed, setDismissed] = useState(false);
  const matches = matchSlash(value);
  // Reset selection + un-dismiss when the typed command text changes — React's
  // "adjust state during render" pattern (no effect, resets before paint).
  const query = matches !== null ? value : "";
  const [prevQuery, setPrevQuery] = useState(query);
  if (query !== prevQuery) {
    setPrevQuery(query);
    setSel(0);
    setDismissed(false);
  }
  const menuOpen = !dismissed && matches !== null && matches.length > 0;
  const selected = Math.min(sel, Math.max(0, (matches?.length ?? 1) - 1));

  useInput(
    (input, key) => {
      if (key.ctrl && input === "c") return onCtrlC();
      if (showKeys) return setShowKeys(false); // any key closes the overlay
      // Picker open: arrows move the selection, ⇥ completes, ↵ runs the
      // highlighted command, esc dismisses — these take over from history/submit.
      if (menuOpen && matches) {
        if (key.upArrow) return setSel((s) => Math.max(0, s - 1));
        if (key.downArrow) return setSel((s) => Math.min(matches.length - 1, s + 1));
        if (key.escape) return setDismissed(true);
        if (key.tab) {
          const pick = matches[selected];
          if (pick) setState(at(`/${pick.name} `, pick.name.length + 2));
          return;
        }
        if (key.return && !key.meta && !key.shift) {
          const pick = matches[selected];
          if (pick) submit(`/${pick.name}`);
          return;
        }
      }
      // `?` on an empty line asks what the keys are, rather than typing a `?`
      // nobody wants — the bindings were documented in a source comment only.
      if (input === "?" && value === "") return setShowKeys(true);

      const next = applyKey(state, input, key);
      if (next === "submit") return submit(value);
      if (next !== null) return setState(next);
      // Unhandled up/down: no line to move to, so they mean history.
      if (key.upArrow) return recall(1);
      if (key.downArrow) return recall(-1);
    },
    { isActive },
  );

  // A solid (non-blinking) block marks the cursor. It used to blink on a 530ms
  // timer, but that re-rendered Ink's live region twice a second — and every
  // live-region repaint is written at the bottom of the buffer, snapping the
  // terminal back down whenever you'd scrolled up to read history (the scroll
  // "glitch"). A static caret repaints only on real input/state changes.
  const caretBlock = (ch: string) =>
    isActive ? (
      <Text color="#000000" backgroundColor={palette.teal}>
        {ch}
      </Text>
    ) : (
      <Text color={palette.white}>{ch}</Text>
    );

  if (showKeys) {
    return (
      <Box flexDirection="column">
        <Text color={palette.teal}>keys</Text>
        {BINDINGS.map(([keys, what]) => (
          <Text key={keys}>
            {"  "}
            <Text color={palette.white}>{keys.padEnd(16)}</Text>
            <Text color={palette.grey}>{what}</Text>
          </Text>
        ))}
        <Text color={palette.grey}> any key to close</Text>
      </Box>
    );
  }

  // The whole input is ONE <Text> (with the caret nested) so Ink wraps it as
  // continuous text: a long line flows onto the next row and the cursor tracks
  // across the wrap. (Three sibling <Text> in a row Box do NOT reflow — that
  // stranded the cursor on line 1 for multi-line input.)
  return (
    <Box flexDirection="column">
      {menuOpen && matches ? <CommandMenu items={matches} selected={selected} /> : null}
      <Text>
        <Text color={palette.teal}>❯ </Text>
        {value === "" ? (
          <>
            {caretBlock(" ")}
            <Text color={palette.grey}>{placeholder}</Text>
          </>
        ) : (
          <>
            <Text color={palette.white}>{value.slice(0, pos)}</Text>
            {/* On a line break the caret shows as a space — painting the "\n"
                itself would swallow the break and collapse the two lines. */}
            {caretBlock(value.slice(pos, pos + 1).replace("\n", " ") || " ")}
            <Text color={palette.white}>{value.slice(pos + 1)}</Text>
          </>
        )}
      </Text>
      {menuOpen ? (
        <Text color={palette.grey}> ↑↓ select · ⇥ complete · ↵ run · esc dismiss</Text>
      ) : null}
    </Box>
  );
}
