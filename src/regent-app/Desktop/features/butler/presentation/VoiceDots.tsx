'use client';
// Regent's voice mark — a port of regent-web's BrailleVoiceViz: a mirrored
// braille-style dot field drawn on canvas at 60fps from the analyser, with a
// GSAP-breathed idle floor so it never looks dead when silent. Colors derive
// from the --accent token (the source hardcoded teals). The analyser arrives
// via ref because Butler creates it after mic setup, without a re-render.
import { useEffect, useRef } from 'react';
import gsap from 'gsap';

const COLS = 28; // columns, mirrored around centre into a symmetric voiceprint
const ROWS = 8; // dots per half-column (×2 mirrored = 16 tall)
const DOT = 5; // dot diameter (logical px, pre-scale)
const GAPX = 11; // column pitch
const GAPY = 9; // row pitch

interface Props {
  analyserRef: React.RefObject<AnalyserNode | null>;
  /** Brighter dots when Regent is the one speaking. */
  speaking?: boolean;
  /** Logical-pixel multiplier — the full-screen Butler runs larger. */
  scale?: number;
}

export function VoiceDots({ analyserRef, speaking = false, scale = 1 }: Props) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const floorRef = useRef({ v: 0.06 }); // idle baseline, breathed by GSAP

  useEffect(() => {
    if (matchMedia('(prefers-reduced-motion: reduce)').matches) {
      floorRef.current.v = 0.1; // static floor — voice levels still render
      return;
    }
    const tween = gsap.to(floorRef.current, {
      // Higher ceiling than the regent-web source (0.17): on the light bone
      // bg the idle breathe was reading as static.
      v: 0.26,
      duration: 1.6,
      ease: 'sine.inOut',
      yoyo: true,
      repeat: -1,
    });
    return () => {
      tween.kill();
    };
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext('2d');
    if (!canvas || !ctx) return;

    const dot = DOT * scale;
    const gapX = GAPX * scale;
    const gapY = GAPY * scale;
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const W = COLS * gapX;
    const H = ROWS * 2 * gapY;
    canvas.width = W * dpr;
    canvas.height = H * dpr;
    canvas.style.width = `${W}px`;
    canvas.style.height = `${H}px`;
    ctx.scale(dpr, dpr);

    // Gradient stops from the token: light (accent mixed toward white) at the
    // centre rows → pure accent at the tips; off-dots are accent at 10%.
    const accent = parseHex(
      getComputedStyle(document.documentElement).getPropertyValue('--accent').trim(),
    );
    const light = mix(accent, [255, 255, 255], 0.55);

    let bins = new Uint8Array(0);
    let raf = 0;

    const draw = () => {
      raf = requestAnimationFrame(draw);
      ctx.clearRect(0, 0, W, H);
      const analyser = analyserRef.current;
      if (analyser && bins.length !== analyser.frequencyBinCount) {
        bins = new Uint8Array(analyser.frequencyBinCount);
      }
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
        const x = c * gapX + gapX / 2;
        for (let r = 0; r < ROWS; r++) {
          const on = r < lit;
          const y1 = midY - (r + 0.5) * gapY;
          const y2 = midY + (r + 0.5) * gapY;
          // Radial falloff so the field melts into the page instead of ending
          // in a hard rectangle (the source sat on a dark bg where off-dots
          // were invisible; on bone they need to fade out toward the edges).
          ctx.fillStyle = dotColor(r / ROWS, on, speaking, accent, light, fade(x, y1, W, H));
          drawDot(ctx, x, y1, on, dot, scale);
          ctx.fillStyle = dotColor(r / ROWS, on, speaking, accent, light, fade(x, y2, W, H));
          drawDot(ctx, x, y2, on, dot, scale);
        }
      }
    };
    draw();
    return () => cancelAnimationFrame(raf);
  }, [analyserRef, speaking, scale]);

  return (
    <canvas ref={canvasRef} aria-hidden className="motion-safe:animate-[fadeIn_400ms_ease-out]" />
  );
}

function drawDot(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  on: boolean,
  dot: number,
  scale: number,
) {
  ctx.beginPath();
  ctx.arc(x, y, on ? dot / 2 : dot / 2 - 1.4 * scale, 0, Math.PI * 2);
  ctx.fill();
}

type Rgb = readonly [number, number, number];

/** 1 at the centre → 0 at the field's elliptical edge (smooth). */
function fade(x: number, y: number, w: number, h: number): number {
  const dx = (x - w / 2) / (w / 2);
  const dy = (y - h / 2) / (h / 2);
  const d = Math.sqrt(dx * dx + dy * dy);
  const t = Math.min(1, Math.max(0, (d - 0.35) / 0.65));
  return 1 - t * t * (3 - 2 * t); // smoothstep out
}

// light → accent gradient down the column; brighter when Regent speaks;
// alpha scaled by the radial fade so edges dissolve into the background.
function dotColor(
  t: number,
  on: boolean,
  speaking: boolean,
  accent: Rgb,
  light: Rgb,
  edge: number,
): string {
  if (!on) return `rgba(${accent[0]},${accent[1]},${accent[2]},${(0.07 * edge * edge).toFixed(3)})`;
  const a = (speaking ? 1 : 0.85) * (0.35 + 0.65 * edge);
  const r = Math.round(light[0] + (accent[0] - light[0]) * t);
  const g = Math.round(light[1] + (accent[1] - light[1]) * t);
  const b = Math.round(light[2] + (accent[2] - light[2]) * t);
  return `rgba(${r},${g},${b},${a.toFixed(3)})`;
}

function parseHex(hex: string): Rgb {
  const h = hex.replace('#', '');
  const full = h.length === 3 ? h.split('').map((c) => c + c).join('') : h;
  return [
    Number.parseInt(full.slice(0, 2), 16) || 0,
    Number.parseInt(full.slice(2, 4), 16) || 0,
    Number.parseInt(full.slice(4, 6), 16) || 0,
  ];
}

function mix(a: Rgb, b: Rgb, t: number): Rgb {
  return [
    Math.round(a[0] + (b[0] - a[0]) * t),
    Math.round(a[1] + (b[1] - a[1]) * t),
    Math.round(a[2] + (b[2] - a[2]) * t),
  ];
}
