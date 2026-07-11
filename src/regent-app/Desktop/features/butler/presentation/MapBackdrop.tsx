'use client';
// The globe — a full-screen dark earth behind the Butler stage, rendered with
// `cobe` (a tiny self-contained WebGL globe: its world map is built in, so it
// needs no tile servers and no CSP allowances, and paints instantly offline).
// When a place is asked for it drops a glowing marker and spins that meridian
// to face the viewer; a labelled pill names each place. The exit fade lives in
// ButlerView's wrapper.
import { useEffect, useRef, useState } from 'react';
import createGlobe, { type COBEOptions, type Marker } from 'cobe';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { CloseIcon } from '@/shared/ui/icons';
import { type GeoHit, geocodePlace } from '@/features/butler/data/geocode';

// cobe's published types omit onRender (its per-frame mutable state); the intersection
// restores it so we stay fully typed without a cast.
interface RenderState {
  phi: number;
  theta: number;
  width: number;
  height: number;
}
type GlobeOptions = COBEOptions & { onRender: (state: RenderState) => void };

const DEG = Math.PI / 180;
const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

export function MapBackdrop({
  places,
  onDismiss,
}: {
  places: readonly string[];
  onDismiss: () => void;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const globeRef = useRef<ReturnType<typeof createGlobe> | null>(null);
  // Rotation the render loop eases toward (a place's meridian, or a slow drift).
  const phi = useRef(0);
  const theta = useRef(0.2);
  const targetPhi = useRef(0);
  const targetTheta = useRef(0.2);
  const autoSpin = useRef(true);
  const sizeRef = useRef({ w: 0, h: 0 });
  const [hits, setHits] = useState<readonly GeoHit[]>([]);
  const [ready, setReady] = useState(false);

  // Build / rebuild the globe when the container has a size. onRender reads the
  // rotation refs each frame, so markers/rotation update without recreation.
  useEffect(() => {
    const canvas = canvasRef.current;
    const container = containerRef.current;
    if (!canvas || !container) return;
    const dpr = Math.min(window.devicePixelRatio || 1, 2);

    const build = () => {
      const w = container.clientWidth;
      const h = container.clientHeight;
      sizeRef.current = { w, h };
      const opts: GlobeOptions = {
        devicePixelRatio: dpr,
        width: w * dpr,
        height: h * dpr,
        phi: phi.current,
        theta: theta.current,
        dark: 1,
        diffuse: 1.2,
        mapSamples: 16000,
        mapBrightness: 6,
        baseColor: [0.18, 0.28, 0.5],
        markerColor: [1, 0.72, 0.24],
        glowColor: [0.25, 0.45, 0.85],
        markers: [],
        onRender: (state) => {
          if (autoSpin.current) targetPhi.current += 0.0016;
          phi.current += (targetPhi.current - phi.current) * 0.08;
          theta.current += (targetTheta.current - theta.current) * 0.08;
          state.phi = phi.current;
          state.theta = theta.current;
          state.width = sizeRef.current.w * dpr;
          state.height = sizeRef.current.h * dpr;
        },
      };
      globeRef.current?.destroy();
      globeRef.current = createGlobe(canvas, opts);
      requestAnimationFrame(() => setReady(true)); // first frame painted
    };

    build();
    const ro = new ResizeObserver(() => {
      if (container.clientWidth !== sizeRef.current.w || container.clientHeight !== sizeRef.current.h) build();
    });
    ro.observe(container);
    return () => {
      ro.disconnect();
      globeRef.current?.destroy();
      globeRef.current = null;
    };
  }, []);

  // Geocode each place, drop a marker, and aim the globe at the first hit.
  useEffect(() => {
    let stale = false;
    void (async () => {
      const found: GeoHit[] = [];
      for (const place of places) {
        const hit = await geocodePlace(place);
        if (stale) return;
        if (hit) found.push(hit);
      }
      if (stale) return;
      setHits(found);
      const markers: Marker[] = found.map((h) => ({ location: [h.lat, h.lon], size: 0.09 }));
      globeRef.current?.update({ markers });
      if (found.length > 0) {
        // Face the first place; cobe's phi is a longitude rotation.
        autoSpin.current = false;
        targetPhi.current = -found[0].lon * DEG;
        targetTheta.current = clamp(found[0].lat * DEG, -0.9, 0.9);
      } else {
        autoSpin.current = true;
      }
    })();
    return () => {
      stale = true;
    };
  }, [places]);

  return (
    <div ref={containerRef} className="absolute inset-0 overflow-hidden" style={{ backgroundColor: '#05060a' }}>
      <canvas
        ref={canvasRef}
        role="img"
        aria-label={t().butler.mapDismiss}
        className={`size-full transition-opacity duration-700 ${ready ? 'opacity-100' : 'opacity-0'}`}
      />
      {/* Faint space vignette at the very edges — the glowing globe is the star. */}
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0"
        style={{ background: 'radial-gradient(ellipse 92% 88% at 50% 50%, transparent 62%, rgba(5,6,10,0.7) 100%)' }}
      />
      {/* Place labels — pill chips naming what's pinned (not 3D-tracked; the
          glowing markers carry the exact positions). */}
      {hits.length > 0 && (
        <div className="pointer-events-none absolute inset-x-0 top-24 flex flex-wrap justify-center gap-2 px-6">
          {hits.map((h) => (
            <span
              key={`${h.lat},${h.lon}`}
              className="rounded-full border border-stroke-primary bg-surface/90 px-3 py-1 text-xs font-semibold text-text-primary shadow-elev backdrop-blur"
            >
              {h.label.split(',')[0]}
            </span>
          ))}
        </div>
      )}
      <div className="absolute left-1/2 top-14 -translate-x-1/2">
        <Button variant="secondary" size="sm" aria-label={t().butler.mapDismiss} onClick={onDismiss}>
          <CloseIcon className="size-3.5" />
          {t().butler.mapDismiss}
        </Button>
      </div>
    </div>
  );
}
