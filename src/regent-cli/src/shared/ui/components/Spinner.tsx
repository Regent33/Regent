import { palette } from "@shared/ui/tokens/theme.ts";
// A small braille spinner in the brand teal. Frame-driven on an interval —
// the published-Ink analog of the reference's use-animation-frame pattern.
import { Text } from "ink";
import { useEffect, useState } from "react";

const FRAMES = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"] as const;

export function Spinner() {
  const [frame, setFrame] = useState(0);
  useEffect(() => {
    const id = setInterval(() => setFrame((f) => (f + 1) % FRAMES.length), 80);
    return () => clearInterval(id);
  }, []);
  return <Text color={palette.teal}>{FRAMES[frame]}</Text>;
}
