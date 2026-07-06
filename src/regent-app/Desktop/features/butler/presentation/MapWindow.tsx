'use client';
// Map window content — MapLibre over OSM raster tiles (CSP-allowed origins),
// place search via Nominatim. Fluid pan/zoom; flyTo on a search hit.
import { useEffect, useRef } from 'react';
import maplibregl from 'maplibre-gl';
import { t } from '@/shared/i18n/t';
import { SearchField } from '@/shared/ui/SearchField';

const OSM_STYLE: maplibregl.StyleSpecification = {
  version: 8,
  sources: {
    osm: {
      type: 'raster',
      tiles: ['https://tile.openstreetmap.org/{z}/{x}/{y}.png'],
      tileSize: 256,
      attribution: '© OpenStreetMap contributors',
    },
  },
  layers: [{ id: 'osm', type: 'raster', source: 'osm' }],
};

export function MapWindow() {
  const mountRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<maplibregl.Map | null>(null);

  useEffect(() => {
    const mount = mountRef.current;
    if (!mount) return;
    const map = new maplibregl.Map({
      container: mount,
      style: OSM_STYLE,
      center: [121.05, 14.6], // ponytail: fixed initial view; geolocation later
      zoom: 9,
      attributionControl: { compact: true },
    });
    mapRef.current = map;
    return () => {
      mapRef.current = null;
      map.remove();
    };
  }, []);

  const search = async (query: string) => {
    if (query.trim() === '' || !mapRef.current) return;
    try {
      const res = await fetch(
        `https://nominatim.openstreetmap.org/search?format=json&limit=1&q=${encodeURIComponent(query)}`,
      );
      const hits = (await res.json()) as Array<{ lat?: string; lon?: string }>;
      const hit = hits[0];
      if (hit?.lat !== undefined && hit.lon !== undefined) {
        mapRef.current.flyTo({ center: [Number(hit.lon), Number(hit.lat)], zoom: 12 });
      }
    } catch {
      // Offline / blocked — the map itself keeps working.
    }
  };

  const s = t().butler.windows;
  return (
    <div className="flex flex-col gap-2">
      <SearchField
        label={s.searchPlace}
        placeholder={s.searchPlace}
        onKeyDown={(e) => {
          if (e.key === 'Enter') void search((e.target as HTMLInputElement).value);
        }}
      />
      <div ref={mountRef} className="h-[240px] w-full overflow-hidden rounded-md" />
    </div>
  );
}
