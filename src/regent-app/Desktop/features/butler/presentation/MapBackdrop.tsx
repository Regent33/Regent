'use client';
// Full-bleed map behind the Butler stage — appears when a place is asked for,
// flies in tilted (pitch 60° reads as 3D) and keeps the faded aesthetic: a
// bone veil + edge mask over the tiles, opacity-transitioned in.
import { useEffect, useRef, useState } from 'react';
import maplibregl from 'maplibre-gl';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { CloseIcon } from '@/shared/ui/icons';
import { geocodePlace } from '@/features/butler/data/geocode';

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

export function MapBackdrop({ query, onDismiss }: { query: string; onDismiss: () => void }) {
  const mountRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<maplibregl.Map | null>(null);
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    const mount = mountRef.current;
    if (!mount) return;
    const map = new maplibregl.Map({
      container: mount,
      style: OSM_STYLE,
      center: [0, 20],
      zoom: 1.6,
      pitch: 0,
      attributionControl: { compact: true },
      interactive: true,
    });
    mapRef.current = map;
    // Fade the whole backdrop in once tiles start painting.
    map.once('load', () => setVisible(true));
    return () => {
      mapRef.current = null;
      map.remove();
    };
  }, []);

  useEffect(() => {
    let stale = false;
    void geocodePlace(query).then((center) => {
      if (stale || center === null || !mapRef.current) return;
      const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
      mapRef.current.flyTo({
        center,
        zoom: 13.5,
        pitch: 58,
        bearing: 18,
        duration: reduced ? 0 : 4200,
        essential: true,
      });
    });
    return () => {
      stale = true;
    };
  }, [query]);

  return (
    <div
      className={`absolute inset-0 transition-opacity duration-700 ease-out ${
        visible ? 'opacity-100' : 'opacity-0'
      }`}
    >
      <div ref={mountRef} className="absolute inset-0" />
      {/* The faded aesthetic: bone veil + soft edge mask over the tiles. */}
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 bg-bg opacity-35"
        style={{
          maskImage: 'radial-gradient(ellipse 90% 85% at 50% 45%, transparent 40%, black 95%)',
          WebkitMaskImage: 'radial-gradient(ellipse 90% 85% at 50% 45%, transparent 40%, black 95%)',
        }}
      />
      <div className="absolute left-1/2 top-14 -translate-x-1/2">
        <Button variant="secondary" size="sm" onClick={onDismiss}>
          <CloseIcon className="size-3.5" />
          {t().butler.mapDismiss}
        </Button>
      </div>
    </div>
  );
}
