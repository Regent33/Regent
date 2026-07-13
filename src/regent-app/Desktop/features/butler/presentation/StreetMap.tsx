'use client';
// The street layer — a detailed dark MapLibre tile map (real roads, labels,
// POIs from CARTO's dark raster tiles) that fades in OVER the globe once it has
// flown to a place. The globe (globe.gl) owns the world/fly-in "wow"; this owns
// the close-up the globe's single earth texture can't show. Raster tiles are
// fetched over the network (connect-src allows *.cartocdn.com).
import { useEffect, useRef } from 'react';
import maplibregl, { type StyleSpecification } from 'maplibre-gl';
import 'maplibre-gl/dist/maplibre-gl.css';
import { type GeoHit, validBbox } from '@/features/butler/data/geocode';

// Dark raster basemap (CARTO). Raster keeps the CSP to one tile host (no vector
// style fetch) and reads well on the dark stage. Attribution required.
const STYLE: StyleSpecification = {
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
      attribution: '© OpenStreetMap © CARTO',
    },
  },
  layers: [{ id: 'carto', type: 'raster', source: 'carto' }],
};

const STREET_ZOOM = 15; // building/street scale — where roads + POI labels show

export function StreetMap({ hit }: { hit: GeoHit }) {
  const mountRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const mount = mountRef.current;
    if (!mount) return;
    // Fit what was actually asked for: the bbox makes "the Philippines" fill
    // the frame with the whole country while "Rizal Park" lands at building
    // scale — the old fixed street zoom showed an arbitrary block of whatever
    // sat at a big place's centroid. No bbox, or a degenerate one (a stray
    // Nominatim result with south>=north/west>=east would hand MapLibre an
    // invalid box and silently fail to render ANYTHING) → the old point+zoom.
    const box = validBbox(hit.bbox);
    const bounds: maplibregl.LngLatBoundsLike | undefined = box
      ? [
          [box[2], box[0]],
          [box[3], box[1]],
        ]
      : undefined;
    const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
    const map = new maplibregl.Map({
      container: mount,
      style: STYLE,
      ...(bounds
        ? { bounds, fitBoundsOptions: { padding: reduced ? 48 : 96, maxZoom: reduced ? 17 : STREET_ZOOM } }
        : { center: [hit.lon, hit.lat] as [number, number], zoom: reduced ? STREET_ZOOM : 12.5 }),
      attributionControl: { compact: true },
      dragRotate: false,
    });
    map.addControl(new maplibregl.NavigationControl({ showCompass: false }), 'bottom-right');
    // Cinematic settle-in: appear a touch wide (continuing the globe's fly),
    // then ease the last stretch — POIs sink to zoom 17, cities/countries
    // just tighten their fit. Reduced motion starts at the final frame instead.
    if (!reduced) {
      map.once('load', () => {
        if (bounds) map.fitBounds(bounds, { padding: 48, maxZoom: 17, duration: 2400 });
        else map.flyTo({ zoom: STREET_ZOOM + 1.5, duration: 2400 });
      });
    }
    new maplibregl.Marker({ color: '#ffb42a' })
      .setLngLat([hit.lon, hit.lat])
      .setPopup(new maplibregl.Popup({ offset: 24, closeButton: false }).setText(hit.label.split(',')[0]))
      .addTo(map)
      .togglePopup();
    const ro = new ResizeObserver(() => map.resize());
    ro.observe(mount);
    return () => {
      ro.disconnect();
      map.remove();
    };
  }, [hit]);

  return (
    <div className="absolute inset-0 motion-safe:animate-[fadeIn_700ms_ease-out]">
      <div ref={mountRef} className="absolute inset-0" />
    </div>
  );
}
