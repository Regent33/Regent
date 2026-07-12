'use client';
// The street layer — a detailed dark MapLibre tile map (real roads, labels,
// POIs from CARTO's dark raster tiles) that fades in OVER the globe once it has
// flown to a place. The globe (globe.gl) owns the world/fly-in "wow"; this owns
// the close-up the globe's single earth texture can't show. Raster tiles are
// fetched over the network (connect-src allows *.cartocdn.com).
import { useEffect, useRef } from 'react';
import maplibregl, { type StyleSpecification } from 'maplibre-gl';
import 'maplibre-gl/dist/maplibre-gl.css';
import type { GeoHit } from '@/features/butler/data/geocode';

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
    const map = new maplibregl.Map({
      container: mount,
      style: STYLE,
      center: [hit.lon, hit.lat],
      zoom: STREET_ZOOM,
      attributionControl: { compact: true },
      dragRotate: false,
    });
    map.addControl(new maplibregl.NavigationControl({ showCompass: false }), 'bottom-right');
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
