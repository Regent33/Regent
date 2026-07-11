'use client';
// The globe — a full-screen photorealistic earth behind the Butler stage,
// rendered with globe.gl (three.js). Textures are bundled locally (public/globe),
// so it needs no tile servers and no CSP allowances and works offline. Drag to
// rotate, wheel to zoom; when a place is asked for it flies the camera there and
// drops a tracked pill label. The exit fade lives in ButlerView's wrapper.
import { useEffect, useRef } from 'react';
import Globe, { type GlobeInstance } from 'globe.gl';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { CloseIcon } from '@/shared/ui/icons';
import { type GeoHit, geocodePlace } from '@/features/butler/data/geocode';

// Accessors run against the htmlElementsData rows (GeoHit); globe.gl types them
// as `object`, so narrow here.
const latOf = (d: object) => (d as GeoHit).lat;
const lngOf = (d: object) => (d as GeoHit).lon;

// A tracked pill label anchored to the coordinate (rotates with the globe).
function pill(d: object): HTMLDivElement {
  const el = document.createElement('div');
  el.className =
    'pointer-events-none -translate-x-1/2 -translate-y-full whitespace-nowrap rounded-full border border-white/20 bg-black/70 px-2.5 py-1 text-[11px] font-semibold text-white shadow-lg backdrop-blur';
  el.textContent = (d as GeoHit).label.split(',')[0];
  return el;
}

export function MapBackdrop({
  places,
  onDismiss,
}: {
  places: readonly string[];
  onDismiss: () => void;
}) {
  const mountRef = useRef<HTMLDivElement>(null);
  const globeRef = useRef<GlobeInstance | null>(null);

  useEffect(() => {
    const mount = mountRef.current;
    if (!mount) return;
    const globe = new Globe(mount, { animateIn: false })
      .backgroundImageUrl('/globe/night-sky.png')
      .globeImageUrl('/globe/earth-night.jpg')
      .bumpImageUrl('/globe/earth-topology.png')
      .showAtmosphere(true)
      .atmosphereColor('#4a9eff')
      .atmosphereAltitude(0.2)
      .htmlLat(latOf)
      .htmlLng(lngOf)
      .htmlAltitude(0.02)
      .htmlElement(pill)
      .width(mount.clientWidth)
      .height(mount.clientHeight);
    globe.pointOfView({ lat: 15, lng: 0, altitude: 2.5 });
    const controls = globe.controls();
    controls.autoRotate = true;
    controls.autoRotateSpeed = 0.45;
    controls.enableZoom = true;
    controls.enablePan = false;
    controls.minDistance = 180; // don't zoom through the surface
    controls.maxDistance = 600;
    // Stop the idle spin the moment the user grabs the globe.
    const stopSpin = () => {
      controls.autoRotate = false;
    };
    controls.addEventListener('start', stopSpin);
    globeRef.current = globe;

    const ro = new ResizeObserver(() => globe.width(mount.clientWidth).height(mount.clientHeight));
    ro.observe(mount);
    return () => {
      ro.disconnect();
      controls.removeEventListener('start', stopSpin);
      globe._destructor();
      globeRef.current = null;
    };
  }, []);

  // Geocode each place, drop tracked pills, and fly the camera to the first hit.
  useEffect(() => {
    let stale = false;
    void (async () => {
      const globe = globeRef.current;
      if (!globe) return;
      const hits: GeoHit[] = [];
      for (const place of places) {
        const hit = await geocodePlace(place);
        if (stale) return;
        if (hit) hits.push(hit);
      }
      if (stale || !globeRef.current) return;
      globe.htmlElementsData(hits as object[]);
      if (hits.length > 0) {
        globe.controls().autoRotate = false;
        // One place: fly in close. Several: pull back to frame them all.
        globe.pointOfView(
          { lat: hits[0].lat, lng: hits[0].lon, altitude: hits.length > 1 ? 2.2 : 1.3 },
          2200,
        );
      } else {
        globe.controls().autoRotate = true;
      }
    })();
    return () => {
      stale = true;
    };
  }, [places]);

  return (
    <div className="absolute inset-0 overflow-hidden" style={{ backgroundColor: '#05060a' }}>
      <div ref={mountRef} className="absolute inset-0" />
      <div className="absolute left-1/2 top-14 -translate-x-1/2">
        <Button variant="secondary" size="sm" aria-label={t().butler.mapDismiss} onClick={onDismiss}>
          <CloseIcon className="size-3.5" />
          {t().butler.mapDismiss}
        </Button>
      </div>
    </div>
  );
}
