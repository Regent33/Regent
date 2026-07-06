'use client';
// A draggable floating panel (React Spring): borderless + shadow elevation,
// header as the drag handle, click-to-front. Drag is immediate; the settle
// after release is the spring. Reduced motion: everything immediate.
import { animated, useSpring } from '@react-spring/web';
import { useEffect, useRef, type PointerEvent, type ReactNode } from 'react';
import { Button } from '@/shared/ui/Button';
import { CloseIcon } from '@/shared/ui/icons';

const WIDTH = 300;
const MARGIN = 8;

export interface FloatingWindowProps {
  title: string;
  closeLabel: string;
  x: number;
  y: number;
  z: number;
  onFocus: () => void;
  onClose: () => void;
  onMove: (x: number, y: number) => void;
  children: ReactNode;
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

export function FloatingWindow({
  title,
  closeLabel,
  x,
  y,
  z,
  onFocus,
  onClose,
  onMove,
  children,
}: FloatingWindowProps) {
  const [pos, api] = useSpring(() => ({ x, y, config: { tension: 320, friction: 32 } }));
  const drag = useRef<{ sx: number; sy: number; ox: number; oy: number } | null>(null);

  // Reopen at the remembered position (registry owns persistence).
  useEffect(() => {
    void api.start({ x, y, immediate: true });
  }, [api, x, y]);

  const bound = (nx: number, ny: number): [number, number] => [
    clamp(nx, MARGIN, window.innerWidth - WIDTH - MARGIN),
    clamp(ny, MARGIN, window.innerHeight - 80),
  ];

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    e.currentTarget.setPointerCapture(e.pointerId);
    drag.current = { sx: e.clientX, sy: e.clientY, ox: pos.x.get(), oy: pos.y.get() };
    onFocus();
  };
  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) return;
    const [nx, ny] = bound(d.ox + e.clientX - d.sx, d.oy + e.clientY - d.sy);
    void api.start({ x: nx, y: ny, immediate: true });
  };
  const onPointerUp = () => {
    if (!drag.current) return;
    drag.current = null;
    onMove(pos.x.get(), pos.y.get());
  };

  return (
    <animated.div
      role="dialog"
      aria-label={title}
      className="absolute rounded-lg bg-bg motion-safe:animate-[fadeIn_120ms_ease-out]"
      style={{ x: pos.x, y: pos.y, zIndex: z, width: WIDTH, boxShadow: 'var(--shadow-elev)' }}
      onPointerDown={onFocus}
    >
      <div
        className="flex cursor-grab select-none items-center justify-between border-b border-stroke-tertiary py-1 pl-3 pr-1 active:cursor-grabbing"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
      >
        <span className="text-xs font-semibold uppercase tracking-[0.08em] text-text-tertiary">
          {title}
        </span>
        <Button variant="ghost" size="iconSm" aria-label={closeLabel} onClick={onClose}>
          <CloseIcon className="size-3.5" />
        </Button>
      </div>
      <div className="max-h-[320px] overflow-y-auto p-3">{children}</div>
    </animated.div>
  );
}
