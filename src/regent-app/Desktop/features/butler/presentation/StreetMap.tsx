'use client';
// The detail layer — satellite imagery with road/place reference overlays that
// fades in OVER the globe once it has
// flown to a place. The globe (globe.gl) owns the world/fly-in "wow"; this owns
// the close-up the globe's single earth texture can't show. Raster tiles are
// fetched over the network (connect-src allows services.arcgisonline.com).
import { useEffect, useRef } from 'react';
import maplibregl, { type StyleSpecification } from 'maplibre-gl';
import 'maplibre-gl/dist/maplibre-gl.css';
import { type GeoHit, validBbox } from '@/features/butler/data/geocode';

// Tokenless ArcGIS raster services: realistic imagery underneath transparent
// transportation and place labels. The reference services are intentionally
// separate layers, so the satellite detail remains visible instead of being
// crushed by an opaque dark basemap. Attribution is required by Esri.
const STYLE: StyleSpecification = {
  version: 8,
  sources: {
    imagery: {
      type: 'raster',
      tiles: ['https://services.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}'],
      tileSize: 256,
      maxzoom: 19,
      attribution: '© Esri — Sources: Esri, Maxar, Earthstar Geographics, and the GIS User Community',
    },
    transportation: {
      type: 'raster',
      tiles: [
        'https://services.arcgisonline.com/ArcGIS/rest/services/Reference/World_Transportation/MapServer/tile/{z}/{y}/{x}',
      ],
      tileSize: 256,
      maxzoom: 19,
      attribution: 'Powered by Esri',
    },
    places: {
      type: 'raster',
      tiles: [
        'https://services.arcgisonline.com/ArcGIS/rest/services/Reference/World_Boundaries_and_Places/MapServer/tile/{z}/{y}/{x}',
      ],
      tileSize: 256,
      maxzoom: 19,
      attribution: 'Powered by Esri',
    },
  },
  layers: [
    { id: 'space', type: 'background', paint: { 'background-color': '#07101b' } },
    {
      id: 'imagery',
      type: 'raster',
      source: 'imagery',
      paint: {
        'raster-opacity': 1,
        'raster-contrast': 0.08,
        'raster-saturation': 0.08,
        'raster-brightness-min': 0.06,
        'raster-brightness-max': 1,
        'raster-fade-duration': 320,
      },
    },
    {
      id: 'transportation',
      type: 'raster',
      source: 'transportation',
      paint: { 'raster-opacity': 0.9, 'raster-fade-duration': 240 },
    },
    {
      id: 'places',
      type: 'raster',
      source: 'places',
      maxzoom: 14,
      paint: { 'raster-opacity': 0.96, 'raster-fade-duration': 240 },
    },
  ],
};

const STREET_ZOOM = 15; // building/street scale — where roads + place labels show

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
    // Centre the place in the VISIBLE map, not in the raw canvas. The stage's
    // captions, phase label and mic controls occupy the bottom of the screen
    // (and this component's own legibility gradient covers the lowest 28%),
    // so uniform padding parks the subject underneath all of that — it reads
    // as "the place isn't centred". Reserving the bottom band lifts it into
    // the open area. Sized from the mount so it holds at any window height.
    const map = new maplibregl.Map({
      container: mount,
      style: STYLE,
      ...(bounds
        ? { bounds, fitBoundsOptions: { padding: reduced ? 48 : 96, maxZoom: reduced ? 17 : STREET_ZOOM } }
        : { center: [hit.lon, hit.lat] as [number, number], zoom: reduced ? STREET_ZOOM : 12.5 }),
      attributionControl: { compact: true },
      dragRotate: false,
      maxZoom: 19,
    });
    map.addControl(new maplibregl.NavigationControl({ showCompass: false }), 'bottom-right');
    // The bottom of the stage is obscured by the phase label, captions, mic
    // controls and this component's own legibility gradient, so centring on
    // the raw canvas parks the place underneath all of it. `setPadding` is the
    // ONE mechanism for that: it is persistent, so fitBounds, flyTo AND the
    // caller's own zooming all keep the place in the VISIBLE centre. (The Map
    // constructor has no `offset` — that is a camera option, not a map option,
    // and was silently accepted then ignored.) Applied on load, when the mount
    // has been laid out and clientHeight is real; clamped so the reserved band
    // can never swallow the frame and leave nothing to zoom into.
    const reserveObscuredBand = () => {
      const height = mount.clientHeight || 0;
      const bottom = Math.max(0, Math.min(Math.round(height * 0.26), height - 160));
      if (bottom > 0) map.setPadding({ top: 0, left: 0, right: 0, bottom });
    };
    // Cinematic settle-in: appear a touch wide (continuing the globe's fly),
    // then ease the last stretch — POIs sink to zoom 17, cities/countries
    // just tighten their fit. Reduced motion starts at the final frame instead.
    map.once('load', () => {
      reserveObscuredBand();
      if (reduced) return;
      if (bounds) map.fitBounds(bounds, { padding: 48, maxZoom: 17, duration: 2400 });
      else map.flyTo({ zoom: STREET_ZOOM + 1.5, duration: 2400 });
    });
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
    // MapLibre's popup rule sets `background:#fff` but NO color, so the label
    // inherited the app's dark-theme text colour and rendered white on white —
    // an invisible marker label. Pin the colour to the popup's own white
    // background (the tip is white too, so nothing else needs restyling).
    <div className="absolute inset-0 motion-safe:animate-[fadeIn_700ms_ease-out] [&_.maplibregl-popup-content]:font-medium [&_.maplibregl-popup-content]:text-[#0b1220]">
      {/* MapLibre adds `.maplibregl-map { position: relative }` to this same
          node, which can override Tailwind's `absolute` rule and collapse an
          otherwise empty mount to 0px. Inline layout wins that cascade. */}
      <div ref={mountRef} style={{ position: 'absolute', inset: 0 }} />
      {/* Keep captions legible without dimming the actual map. */}
      <div
        aria-hidden
        className="pointer-events-none absolute inset-x-0 bottom-0 h-[28%]"
        style={{ background: 'linear-gradient(to bottom, transparent, rgba(5, 8, 13, 0.62))' }}
      />
    </div>
  );
}
