'use client';
// The globe — a full-screen photorealistic earth behind the Butler stage,
// rendered with globe.gl (three.js). Textures are bundled locally (public/globe),
// so it needs no tile servers and no CSP allowances and works offline. Drag to
// rotate, wheel to zoom; when a place is asked for it flies the camera there and
// marks it with a glowing point + label — both placed by globe.gl's own geo math
// (same as the camera), so they always land on the real spot. Exit fade lives in
// ButlerView's wrapper.
import { useEffect, useRef } from 'react';
import Globe, { type GlobeInstance } from 'globe.gl';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { CloseIcon } from '@/shared/ui/icons';
import { type GeoHit, geocodePlace } from '@/features/butler/data/geocode';

// Accessors run against the layer rows (GeoHit); globe.gl types them as `object`.
const latOf = (d: object) => (d as GeoHit).lat;
const lngOf = (d: object) => (d as GeoHit).lon;
const textOf = (d: object) => (d as GeoHit).label.split(',')[0];

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
      .globeImageUrl('/globe/earth-day.jpg')
      .bumpImageUrl('/globe/earth-topology.png')
      .showAtmosphere(true)
      .atmosphereColor('#7ab8ff')
      .atmosphereAltitude(0.18)
      // Glowing marker point at the exact coordinate.
      .pointLat(latOf)
      .pointLng(lngOf)
      .pointColor(() => '#ffb42a')
      .pointAltitude(0.02)
      .pointRadius(0.5)
      // Text label beside the marker (rendered in-scene at the same point).
      .labelLat(latOf)
      .labelLng(lngOf)
      .labelText(textOf)
      .labelColor(() => '#ffffff')
      .labelSize(1.1)
      .labelDotRadius(0.35)
      .labelAltitude(0.02)
      .labelResolution(2)
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
    const stopSpin = () => {
      controls.autoRotate = false;
    };
    controls.addEventListener('start', stopSpin); // idle spin stops on grab
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

  // Geocode each place, mark it, and fly the camera to the first hit.
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
      globe.pointsData(hits as object[]).labelsData(hits as object[]);
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
