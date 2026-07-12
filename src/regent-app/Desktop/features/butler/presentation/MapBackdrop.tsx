'use client';
// The globe — a full-screen photorealistic earth (globe.gl / three.js) behind
// the Butler stage. Textures are bundled locally (public/globe), so the WORLD
// view needs no tile servers and works offline. Drag to rotate, wheel to zoom;
// when a place is asked for it flies the camera there and marks it with a
// glowing point + label. The globe owns the world/fly-in "wow" — for the
// close-up (streets, POIs), once it has flown in it hands off to a detailed
// MapLibre StreetMap that fades in on top (a single earth texture pixelates past
// city scale; tiles don't). Exit fade lives in ButlerView's wrapper.
import { useEffect, useRef, useState } from 'react';
import Globe, { type GlobeInstance } from 'globe.gl';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { CloseIcon } from '@/shared/ui/icons';
import { type GeoHit, geocodePlace } from '@/features/butler/data/geocode';
import { StreetMap } from '@/features/butler/presentation/StreetMap';

const FLY_MS = 2200; // globe fly-in; the street map fades in just after it lands
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
  // The place to show streets for; null = globe only (world view / no hit yet).
  const [detail, setDetail] = useState<GeoHit | null>(null);
  // The places we've already flown to — so the same turn re-raising `places`
  // (a fresh array, identical content) doesn't reset the hand-off mid-flight.
  const flownKeyRef = useRef('');

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
      .polygonCapColor(() => 'rgba(0,0,0,0)')
      .polygonSideColor(() => 'rgba(0,0,0,0)')
      .polygonStrokeColor(() => 'rgba(150,190,255,0.55)')
      .polygonAltitude(0.006)
      .pointLat(latOf)
      .pointLng(lngOf)
      .pointColor(() => '#ffb42a')
      .pointAltitude(0.02)
      .pointRadius(0.5)
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
    controls.minDistance = 101;
    controls.maxDistance = 600;
    controls.zoomSpeed = 1.4;
    const stopSpin = () => {
      controls.autoRotate = false;
    };
    controls.addEventListener('start', stopSpin);
    globeRef.current = globe;

    void fetch('/globe/countries.geojson')
      .then((r) => r.json())
      .then((geo: { features?: object[] }) => {
        globeRef.current?.polygonsData(geo.features ?? []);
      })
      .catch(() => {});

    const ro = new ResizeObserver(() => globe.width(mount.clientWidth).height(mount.clientHeight));
    ro.observe(mount);
    return () => {
      ro.disconnect();
      controls.removeEventListener('start', stopSpin);
      globe._destructor();
      globeRef.current = null;
    };
  }, []);

  // Geocode each place, mark it, fly the globe to the first hit, and — for a
  // single place — hand off to the detailed street map once it has landed.
  useEffect(() => {
    const key = places.join('|');
    if (key === flownKeyRef.current) return; // same places re-raised → don't churn
    flownKeyRef.current = key;
    let stale = false;
    let handoff: ReturnType<typeof setTimeout> | undefined;
    setDetail(null); // new place: back to the globe until it flies in again
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
      if (hits.length === 0) {
        globe.controls().autoRotate = true;
        return;
      }
      // Fly to the primary place, then hand off to its detailed street map once
      // the globe has landed. (Extra near-duplicate hits from one query are
      // pinned on the globe during the fly; the street map is the payoff.)
      globe.controls().autoRotate = false;
      globe.pointOfView({ lat: hits[0].lat, lng: hits[0].lon, altitude: 0.5 }, FLY_MS);
      handoff = setTimeout(() => !stale && setDetail(hits[0]), FLY_MS + 150);
    })();
    return () => {
      stale = true;
      if (handoff) clearTimeout(handoff);
    };
  }, [places]);

  // Performance: only one WebGL surface renders at a time. When the street map
  // (maplibre) covers the globe, stop the globe's three.js render loop; resume
  // it when we come back. Without this, both would render every frame.
  useEffect(() => {
    const globe = globeRef.current;
    if (!globe) return;
    if (detail) globe.pauseAnimation();
    else globe.resumeAnimation();
  }, [detail]);

  return (
    <div className="absolute inset-0 overflow-hidden" style={{ backgroundColor: '#05060a' }}>
      <div ref={mountRef} className="absolute inset-0" />
      {detail && <StreetMap hit={detail} />}
      <div className="absolute left-1/2 top-14 z-10 -translate-x-1/2">
        {detail ? (
          <Button variant="secondary" size="sm" onClick={() => setDetail(null)}>
            <CloseIcon className="size-3.5" />
            {t().butler.mapBackToGlobe}
          </Button>
        ) : (
          <Button variant="secondary" size="sm" aria-label={t().butler.mapDismiss} onClick={onDismiss}>
            <CloseIcon className="size-3.5" />
            {t().butler.mapDismiss}
          </Button>
        )}
      </div>
    </div>
  );
}
