"use client";

import { useEffect, useRef } from "react";
import { animated, useSpring } from "@react-spring/web";
import gsap from "gsap";

// A braille-style dot field (2-wide cells) that reacts to live audio — Regent's
// "talking" animation. Canvas-drawn at 60fps from the analyser (React Spring just
// fades it in; GSAP breathes a baseline so it never looks dead when silent).
const COLS = 28; // columns, mirrored around centre into a symmetric voiceprint
const ROWS = 8; // dots per half-column (×2 mirrored = 16 tall)
const DOT = 5; // dot diameter (logical px)
const GAPX = 11; // column pitch
const GAPY = 9; // row pitch

interface Props {
  analyser: AnalyserNode | null;
  /** Brighter dots when Regent is the one speaking. */
  speaking?: boolean;
  className?: string;
}

export function BrailleVoiceViz({ analyser, speaking = false, className }: Props) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const floorRef = useRef({ v: 0.06 }); // idle baseline, breathed by GSAP

  const spring = useSpring({
    from: { opacity: 0, transform: "translateY(10px)" },
    to: { opacity: 1, transform: "translateY(0px)" },
    config: { tension: 180, friction: 22 },
  });

  useEffect(() => {
    const tween = gsap.to(floorRef.current, {
      v: 0.17,
      duration: 1.6,
      ease: "sine.inOut",
      yoyo: true,
      repeat: -1,
    });
    return () => {
      tween.kill();
    };
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return;

    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const W = COLS * GAPX;
    const H = ROWS * 2 * GAPY;
    canvas.width = W * dpr;
    canvas.height = H * dpr;
    canvas.style.width = `${W}px`;
    canvas.style.height = `${H}px`;
    ctx.scale(dpr, dpr);

    const bins = new Uint8Array(analyser ? analyser.frequencyBinCount : 0);
    let raf = 0;

    const draw = () => {
      raf = requestAnimationFrame(draw);
      ctx.clearRect(0, 0, W, H);
      if (analyser && bins.length) analyser.getByteFrequencyData(bins);

      const floor = floorRef.current.v;
      const midY = H / 2;

      for (let c = 0; c < COLS; c++) {
        const m = c < COLS / 2 ? COLS / 2 - 1 - c : c - COLS / 2; // mirror
        let level = floor;
        if (analyser && bins.length) {
          const lo = Math.floor((m / (COLS / 2)) * 48); // low-mid band carries voice
          const hi = Math.min(lo + 3, bins.length);
          let s = 0;
          for (let i = lo; i < hi; i++) s += bins[i] ?? 0;
          level = Math.max(floor, (s / (hi - lo) / 255) ** 0.8);
        }
        const lit = Math.round(level * ROWS);
        const x = c * GAPX + GAPX / 2;
        for (let r = 0; r < ROWS; r++) {
          const on = r < lit;
          ctx.fillStyle = dotColor(r / ROWS, on, speaking);
          dot(ctx, x, midY - (r + 0.5) * GAPY, on);
          dot(ctx, x, midY + (r + 0.5) * GAPY, on);
        }
      }
    };
    draw();
    return () => cancelAnimationFrame(raf);
  }, [analyser, speaking]);

  return <animated.canvas ref={canvasRef} style={spring} className={className} aria-hidden />;
}

function dot(ctx: CanvasRenderingContext2D, x: number, y: number, on: boolean) {
  ctx.beginPath();
  ctx.arc(x, y, on ? DOT / 2 : DOT / 2 - 1.4, 0, Math.PI * 2);
  ctx.fill();
}

// teal-glow → teal-deep gradient down the column; brighter when Regent speaks.
function dotColor(t: number, on: boolean, speaking: boolean): string {
  if (!on) return "rgba(45,212,191,0.10)";
  const a = speaking ? 1 : 0.85;
  const r = Math.round(94 + (13 - 94) * t);
  const g = Math.round(234 + (148 - 234) * t);
  const b = Math.round(212 + (136 - 212) * t);
  return `rgba(${r},${g},${b},${a})`;
}
