'use client';
// A draggable, optionally resizable floating panel (React Spring): borderless
// + shadow elevation, header as the drag handle, click-to-front, a spring
// pop-in on mount. Drag/resize push geometry through immediately
// (useDragResize owns the pointer-capture gesture); the settle after release
// is the spring. Reduced motion: pop-in is skipped outright (drag/resize were
// already immediate).
import { animated, useSpring } from '@react-spring/web';
import { useEffect, type ReactNode } from 'react';
import { Button } from '@/shared/ui/Button';
import { CloseIcon } from '@/shared/ui/icons';
import { useDragResize } from '@/features/butler/presentation/useDragResize';

export interface FloatingWindowProps {
  title: string;
  closeLabel: string;
  resizeLabel?: string;
  x: number;
  y: number;
  z: number;
  width?: number;
  /** Explicit panel height — omit to keep the body's own `max-h-80` (320px)
   * clamp (the three fixed windows' original, unchanged look). */
  height?: number;
  /** Shows the corner resize grip. Default off so existing callers (the
   * fixed conversation/results/insights windows) are unaffected. */
  resizable?: boolean;
  onFocus: () => void;
  onClose: () => void;
  onMove: (x: number, y: number) => void;
  onResize?: (width: number, height: number) => void;
  children: ReactNode;
}

const reducedMotion = () => matchMedia('(prefers-reduced-motion: reduce)').matches;

export function FloatingWindow({
  title,
  closeLabel,
  resizeLabel,
  x,
  y,
  z,
  width = 300,
  height,
  resizable = false,
  onFocus,
  onClose,
  onMove,
  onResize,
  children,
}: FloatingWindowProps) {
  const [pos, api] = useSpring(() => ({
    x,
    y,
    width,
    height: height ?? 0,
    scale: reducedMotion() ? 1 : 0.96,
    opacity: reducedMotion() ? 1 : 0,
    config: { tension: 320, friction: 32 },
  }));

  // Reopen at the remembered geometry (registry owns persistence).
  useEffect(() => {
    void api.start({ x, y, width, height: height ?? 0, immediate: true });
  }, [api, x, y, width, height]);

  // Pop in once on mount — skipped outright when reduced motion is asked for.
  useEffect(() => {
    void api.start({ scale: 1, opacity: 1, immediate: reducedMotion() });
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once per mount
  }, []);

  const { drag, grip } = useDragResize(
    {
      getX: () => pos.x.get(),
      getY: () => pos.y.get(),
      getWidth: () => pos.width.get(),
      getHeight: () => pos.height.get(),
      setPosition: (nx, ny) => void api.start({ x: nx, y: ny, immediate: true }),
      setSize: (nw, nh) => void api.start({ width: nw, height: nh, immediate: true }),
    },
    onFocus,
    onMove,
    onResize,
  );

  return (
    <animated.div
      role="dialog"
      aria-label={title}
      className="absolute flex flex-col rounded-lg bg-bg"
      style={{
        x: pos.x,
        y: pos.y,
        zIndex: z,
        width: pos.width,
        height: resizable ? pos.height : undefined,
        scale: pos.scale,
        opacity: pos.opacity,
        boxShadow: 'var(--shadow-elev)',
      }}
      onPointerDown={onFocus}
    >
      <div
        className="flex cursor-grab select-none items-center justify-between border-b border-stroke-tertiary py-1 pl-3 pr-1 active:cursor-grabbing"
        {...drag}
      >
        <span className="text-xs font-semibold uppercase tracking-[0.08em] text-text-tertiary">
          {title}
        </span>
        <Button variant="ghost" size="iconSm" aria-label={closeLabel} onClick={onClose}>
          <CloseIcon className="size-3.5" />
        </Button>
      </div>
      <div className={resizable ? 'flex-1 overflow-y-auto p-3' : 'max-h-80 overflow-y-auto p-3'}>
        {children}
      </div>
      {resizable && (
        <div
          aria-label={resizeLabel}
          className="absolute bottom-0.5 right-0.5 size-3 cursor-nwse-resize touch-none opacity-40 hover:opacity-80"
          style={{ background: 'linear-gradient(135deg, transparent 50%, currentColor 50%)' }}
          {...grip}
        />
      )}
    </animated.div>
  );
}
