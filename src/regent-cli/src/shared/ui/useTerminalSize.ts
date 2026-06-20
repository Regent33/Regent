// Reactive terminal dimensions — re-renders the live region when the window
// resizes (Ink reads stdout.columns/rows fresh on each render, but doesn't
// trigger a render on resize by itself).
import { useEffect, useState } from "react";

export interface TerminalSize {
  readonly columns: number;
  readonly rows: number;
}

function current(): TerminalSize {
  return { columns: process.stdout.columns ?? 80, rows: process.stdout.rows ?? 24 };
}

export function useTerminalSize(): TerminalSize {
  const [size, setSize] = useState<TerminalSize>(current);
  useEffect(() => {
    const onResize = () => setSize(current());
    process.stdout.on("resize", onResize);
    return () => {
      process.stdout.off("resize", onResize);
    };
  }, []);
  return size;
}
