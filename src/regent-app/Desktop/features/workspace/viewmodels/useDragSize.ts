'use client';
// Drag-to-resize for the panel and its inner split. A pointer-events drag is
// the whole implementation — no splitter dependency, and `setPointerCapture`
// means the drag survives the pointer leaving the 4px handle.
import { useCallback, useEffect, useRef, useState } from 'react';

/** `size` in px, clamped to [min, max]. `direction: -1` when dragging the
 * LEFT edge of a right-hand panel (moving left must grow it). */
export function useDragSize(initial: number, min: number, max: number, direction: 1 | -1 = 1) {
  const [size, setSize] = useState(initial);
  const start = useRef({ pointer: 0, size: 0 });

  const onPointerDown = useCallback(
    (e: React.PointerEvent<HTMLElement>) => {
      e.preventDefault();
      e.currentTarget.setPointerCapture(e.pointerId);
      start.current = { pointer: e.clientX, size };
      // Without this the drag doubles as a text drag-select: the pointer
      // sweeps across the chat and file tree and highlights everything it
      // crosses. Cleared on pointer-up below.
      document.body.style.userSelect = 'none';
      document.body.style.cursor = 'col-resize';
    },
    [size],
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent<HTMLElement>) => {
      if (!e.currentTarget.hasPointerCapture(e.pointerId)) return;
      const delta = (e.clientX - start.current.pointer) * direction;
      setSize(Math.min(max, Math.max(min, start.current.size + delta)));
    },
    [direction, min, max],
  );

  const onPointerUp = useCallback((e: React.PointerEvent<HTMLElement>) => {
    e.currentTarget.releasePointerCapture(e.pointerId);
    document.body.style.userSelect = '';
    document.body.style.cursor = '';
  }, []);

  // A drag interrupted by unmount (panel closed mid-drag) must not strand the
  // page unselectable.
  useEffect(
    () => () => {
      document.body.style.userSelect = '';
      document.body.style.cursor = '';
    },
    [],
  );

  return { size, setSize, handleProps: { onPointerDown, onPointerMove, onPointerUp } };
}
