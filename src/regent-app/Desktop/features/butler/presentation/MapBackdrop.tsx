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
import { type GeoHit, geocodePlace, validBbox } from '@/features/butler/data/geocode';
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
  // The place we've already flown to, keyed by its RESOLVED COORDINATES —
  // not the raw `places` candidate list. `places` names query VARIANTS of the
  // same ask ("tesla factory, china" AND the bare "tesla factory" fallback),
  // and setHeard's early resolve + the turn-end resolve can legitimately
  // disagree on which variants succeeded (a transient Nominatim hiccup on
  // one of two near-simultaneous lookups isn't cached, so it just retries
  // and can drop or add an entry). Keying on the candidate-array STRING
  // treated that as a brand-new place — resetting `detail` and re-flying
  // right as the street map was about to show, which read as "it never
  // appears". Keying on where hits[0] actually landed is what "the same
  // place" really means here.
  const flownKeyRef = useRef('');
  // The pending globe→street-map hand-off. Lives OUTSIDE the effect's cleanup:
  // the turn-end re-raise of the SAME places re-runs the effect, and a cleanup-
  // owned timer got cleared right there — early-return on the key match never
  // rescheduled it, so the street map never appeared (only the zoomed globe).
  const handoffRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

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
      if (hits.length === 0) {
        globe.controls().autoRotate = true;
        return;
      }
      // Identify by where hits[0] actually landed, decided AFTER resolving —
      // not by the raw `places` list beforehand (see the ref's own comment).
      const flownKey = `${hits[0].lat.toFixed(3)},${hits[0].lon.toFixed(3)}`;
      if (flownKey === flownKeyRef.current) return; // same place already flown/flying — keep it
      flownKeyRef.current = flownKey;
      clearTimeout(handoffRef.current); // a genuinely DIFFERENT place supersedes any pending hand-off
      setDetail(null); // new place: back to the globe until it flies in again
      // Fly to the primary place, then hand off to its detailed street map once
      // the globe has landed. (Extra near-duplicate hits from one query are
      // pinned on the globe during the fly; the street map is the payoff.)
      // Land at an altitude matched to the place's SIZE (bbox span): a country
      // reads whole from ~1.0, a landmark deserves a close 0.18 swoop — one
      // fixed altitude made every landing feel far away. 0.18 floor: the globe
      // texture pixelates closer; the street map owns anything nearer.
      const box = validBbox(hits[0].bbox);
      const span = box ? Math.max(box[1] - box[0], box[3] - box[2]) : 6;
      const altitude = Math.min(1.6, Math.max(0.18, span / 28));
      globe.controls().autoRotate = false;
      globe.pointOfView({ lat: hits[0].lat, lng: hits[0].lon, altitude }, FLY_MS);
      handoffRef.current = setTimeout(() => setDetail(hits[0]), FLY_MS + 150);
    })();
    return () => {
      stale = true;
    };
  }, [places]);

  // Unmount only — a same-key effect re-run must NOT kill the pending hand-off.
  useEffect(() => () => clearTimeout(handoffRef.current), []);

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
