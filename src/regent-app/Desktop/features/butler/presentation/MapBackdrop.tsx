'use client';
// The globe — a full-screen dark earth behind the Butler stage. It spins up
// when a place is asked for: MapLibre's globe projection over CARTO dark-matter
// tiles (glowing city lights on a space-black ocean), a pin per place, and a
// dramatic fly-in. Multiple places frame together; the user then explores
// freely (drag to rotate, wheel to street level). It fades in on load; the
// exit fade lives in ButlerView's wrapper.
import { useEffect, useRef, useState } from 'react';
import maplibregl from 'maplibre-gl';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { CloseIcon } from '@/shared/ui/icons';
import { type GeoHit, geocodePlace } from '@/features/butler/data/geocode';

// Projection is applied after load (map.setProjection) rather than in the style,
// so a globe failure in the webview falls back to a flat map instead of a blank
// canvas. CARTO dark tiles need their hosts in the Tauri CSP (tauri.conf.json) —
// restart the app after a CSP change or they silently 403 and the earth stays
// featureless.
const GLOBE_STYLE: maplibregl.StyleSpecification = {
  version: 8,
  sources: {
    carto: {
      type: 'raster',
      tiles: [
        'https://a.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}.png',
        'https://b.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}.png',
        'https://c.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}.png',
      ],
      tileSize: 256,
      attribution: '© OpenStreetMap contributors © CARTO',
    },
  },
  layers: [
    { id: 'space', type: 'background', paint: { 'background-color': '#05060a' } },
    { id: 'carto', type: 'raster', source: 'carto' },
  ],
};

// A pin: a token-themed pill label anchored to the coordinate, its popup a
// small card with the full place name and lat/lon. Built as DOM (not JSX) so
// MapLibre can own it; the pill is a <button> so it is keyboard-reachable and
// toggles the popup on Enter/click.
function createPin(hit: GeoHit): maplibregl.Marker {
  const pill = document.createElement('button');
  pill.type = 'button';
  pill.className =
    'cursor-pointer rounded-full border border-stroke-primary bg-surface px-2 py-0.5 text-[11px] font-semibold text-text-primary shadow-elev';
  pill.textContent = hit.label.split(',')[0];
  pill.setAttribute('aria-label', hit.label);

  const card = document.createElement('div');
  const name = document.createElement('p');
  name.className = 'text-xs font-semibold text-text-primary';
  name.textContent = hit.label;
  const coords = document.createElement('p');
  coords.className = 'mt-0.5 text-[11px] text-text-tertiary';
  coords.textContent = `${hit.lat.toFixed(4)}, ${hit.lon.toFixed(4)}`;
  card.append(name, coords);

  const popup = new maplibregl.Popup({
    offset: 16,
    closeButton: false,
    className: 'butler-map-popup',
  }).setDOMContent(card);

  return new maplibregl.Marker({ element: pill, anchor: 'bottom' })
    .setLngLat([hit.lon, hit.lat])
    .setPopup(popup);
}

export function MapBackdrop({
  places,
  onDismiss,
}: {
  places: readonly string[];
  onDismiss: () => void;
}) {
  const mountRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<maplibregl.Map | null>(null);
  const markersRef = useRef<maplibregl.Marker[]>([]);
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    const mount = mountRef.current;
    if (!mount) return;
    let map: maplibregl.Map;
    try {
      map = new maplibregl.Map({
        container: mount,
        style: GLOBE_STYLE,
        center: [0, 15],
        zoom: 1.6,
        attributionControl: { compact: true },
        interactive: true,
      });
    } catch (e) {
      // WebGL/context failure — nothing to show, but never trap the caller.
      console.warn('[globe] map init failed', e);
      setVisible(true);
      return;
    }
    mapRef.current = map;
    map.on('load', () => {
      // Globe projection is the goal; a webview that rejects it keeps the flat
      // map rather than a blank canvas.
      try {
        map.setProjection({ type: 'globe' });
      } catch (e) {
        console.warn('[globe] globe projection unsupported, staying flat', e);
      }
      setVisible(true);
    });
    // Surface tile/style errors (CSP blocks, offline) instead of a silent blank.
    map.on('error', (e) => console.warn('[globe]', e.error?.message ?? e));
    // Never leave the backdrop invisible if `load` is slow or never fires.
    const fallback = setTimeout(() => setVisible(true), 1500);
    return () => {
      clearTimeout(fallback);
      for (const m of markersRef.current) m.remove();
      markersRef.current = [];
      mapRef.current = null;
      map.remove();
    };
  }, []);

  useEffect(() => {
    let stale = false;
    void (async () => {
      const map = mapRef.current;
      if (!map) return;
      for (const m of markersRef.current) m.remove();
      markersRef.current = [];
      // Sequential geocoding respects Nominatim's rate limit; pins drop as they
      // resolve, then the camera frames whatever landed.
      const hits: GeoHit[] = [];
      for (const place of places) {
        const hit = await geocodePlace(place);
        if (stale || !mapRef.current) return;
        if (hit) {
          hits.push(hit);
          markersRef.current.push(createPin(hit).addTo(mapRef.current));
        }
      }
      if (stale || hits.length === 0) return;
      const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
      if (hits.length === 1) {
        map.flyTo({
          center: [hits[0].lon, hits[0].lat],
          zoom: 13.5,
          pitch: 58,
          bearing: 18,
          duration: reduced ? 0 : 4200,
          essential: true,
        });
      } else {
        const bounds = new maplibregl.LngLatBounds();
        for (const h of hits) bounds.extend([h.lon, h.lat]);
        map.fitBounds(bounds, {
          padding: 120,
          maxZoom: 6,
          duration: reduced ? 0 : 2600,
          essential: true,
        });
      }
    })();
    return () => {
      stale = true;
    };
  }, [places]);

  return (
    <div
      className={`absolute inset-0 transition-opacity duration-700 ease-out ${
        visible ? 'opacity-100' : 'opacity-0'
      }`}
    >
      <div ref={mountRef} className="absolute inset-0" style={{ backgroundColor: '#05060a' }} />
      {/* The globe is the star now — only a faint space vignette at the edges. */}
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0"
        style={{
          background:
            'radial-gradient(ellipse 88% 82% at 50% 50%, transparent 58%, rgba(5,6,10,0.82) 100%)',
        }}
      />
      <div className="absolute left-1/2 top-14 -translate-x-1/2">
        <Button variant="secondary" size="sm" aria-label={t().butler.mapDismiss} onClick={onDismiss}>
          <CloseIcon className="size-3.5" />
          {t().butler.mapDismiss}
        </Button>
      </div>
    </div>
  );
}
