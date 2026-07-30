'use client';
// Drag-to-resize for the panel and its inner split. A pointer-events drag is
// the whole implementation — no splitter dependency, and `setPointerCapture`
// means the drag survives the pointer leaving the 4px handle.
import { useCallback, useEffect, useRef, useState } from 'react';

/** `size` in px, clamped to [min, max]. `direction: -1` when dragging the
 * LEFT edge of a right-hand panel (moving left must grow it) — or the TOP edge
 * of a bottom panel, where moving up must grow it. `axis: 'y'` for a horizontal
 * handle: one parameter rather than a near-identical second hook, since the only
 * differences are which coordinate is read and which cursor is shown. */
export function useDragSize(
  initial: number,
  min: number,
  max: number,
  direction: 1 | -1 = 1,
  axis: 'x' | 'y' = 'x',
) {
  const [size, setSize] = useState(initial);
  const start = useRef({ pointer: 0, size: 0 });
  const cursor = axis === 'y' ? 'row-resize' : 'col-resize';
  const coordinate = useCallback(
    (e: React.PointerEvent<HTMLElement>) => (axis === 'y' ? e.clientY : e.clientX),
    [axis],
  );

  const onPointerDown = useCallback(
    (e: React.PointerEvent<HTMLElement>) => {
      e.preventDefault();
      e.currentTarget.setPointerCapture(e.pointerId);
      start.current = { pointer: coordinate(e), size };
      // Without this the drag doubles as a text drag-select: the pointer
      // sweeps across the chat and file tree and highlights everything it
      // crosses. Cleared on pointer-up below.
      document.body.style.userSelect = 'none';
      document.body.style.cursor = cursor;
    },
    [size, coordinate, cursor],
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent<HTMLElement>) => {
      if (!e.currentTarget.hasPointerCapture(e.pointerId)) return;
      const delta = (coordinate(e) - start.current.pointer) * direction;
      setSize(Math.min(max, Math.max(min, start.current.size + delta)));
    },
    [coordinate, direction, min, max],
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

  // Shrinking the WINDOW must shrink the panel too. Without this the stored
  // width survives the resize and the chat column gets squeezed to nothing —
  // the drag cap only applies while dragging.
  useEffect(() => {
    const onResize = () => setSize((current) => Math.min(max, Math.max(min, current)));
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, [min, max]);

  return { size, setSize, handleProps: { onPointerDown, onPointerMove, onPointerUp } };
}
