---
name: maps
description: "Geocode, POIs, routes, timezones via OpenStreetMap/OSRM."
version: 1.0.0
created_by: bundled
pinned: true
tags: [maps, geocoding, routing, location, openstreetmap]
---

Location intelligence from free, open data via `terminal` + `curl` — no API
key. Sources: Nominatim (geocoding), Overpass (POIs), OSRM (routing).

**Nominatim usage policy:** max 1 request/second, and every request needs a
`User-Agent` header identifying the app — requests without one get blocked.

## search — geocode a place name
```bash
curl -s -H "User-Agent: regent-maps/1.0" \
  "https://nominatim.openstreetmap.org/search?q=Eiffel+Tower&format=json&limit=5"
```
Returns lat/lon, display name, type, bounding box, importance score.

## reverse — coordinates to address
```bash
curl -s -H "User-Agent: regent-maps/1.0" \
  "https://nominatim.openstreetmap.org/reverse?lat=48.8584&lon=2.2945&format=json"
```
Returns a full address breakdown (street, city, state, country, postcode).

## nearby — POIs by category (Overpass)
Overpass Query Language, `around:RADIUS_M,LAT,LON`, filtered by OSM tag:
```bash
curl -s --data-urlencode 'data=[out:json][timeout:25];
node(around:1000,48.8584,2.2945)[amenity=restaurant];
out body 20;' https://overpass-api.de/api/interpreter
```
Common tags: `amenity=restaurant|cafe|bar|hospital|pharmacy|bank|school|
library|fuel|parking|police|fire_station|dentist|doctors|cinema|nightclub`,
`shop=supermarket|bakery|convenience|bookshop|car_repair`,
`tourism=hotel|museum|attraction`, `leisure=park|fitness_centre|swimming_pool`.
Multiple categories: chain more `node(around:...)[tag=value];` lines before
`out;`.

If the place is named rather than coordinates, geocode it first with
`search`, then feed the lat/lon into the query above.

Each POI in the response has `tags.name`, `lat`/`lon`, and often
`tags.opening_hours`, `tags.phone`, `tags.website`, `tags.cuisine`. Build a
`https://www.google.com/maps?q=LAT,LON` link for the user to tap. OSM hours
are community-maintained — verify "open now?" questions with `web_search`
if the tag is missing or looks stale.

If `overpass-api.de` is slow or down, retry against the mirror
`https://overpass.kumi.systems/api/interpreter`.

## distance / directions — routing (OSRM)
OSRM takes `lon,lat` pairs (longitude first), semicolon-separated:
```bash
# Distance + duration only
curl -s "http://router.project-osrm.org/route/v1/driving/2.2945,48.8584;2.3376,48.8606?overview=false"

# Turn-by-turn steps
curl -s "http://router.project-osrm.org/route/v1/walking/2.2945,48.8584;2.3376,48.8606?steps=true&geometries=geojson"
```
Profile is in the URL path: `driving`, `walking`, `cycling`. Response's
`routes[0].distance` (meters), `routes[0].duration` (seconds), and — with
`steps=true` — `legs[0].steps[]` with `distance`, `duration`, `name`
(road), and `maneuver.type`.

Geocode both endpoints with `search` first if the user gave place names, not
coordinates. OSRM's public router has best coverage in Europe and North
America; sparser elsewhere.

## timezone
Prefer the built-in `world_time` tool if it's in your catalog. Otherwise:
```bash
curl -s "https://www.timeapi.io/api/Time/current/coordinate?latitude=48.8584&longitude=2.2945"
```
Returns timezone name, UTC offset, current local time.

## area / bbox — search within a region
Get a bounding box from Nominatim's `search` response (`boundingbox` field:
`[south, north, west, east]`), then query Overpass with a bbox filter
instead of `around`:
```bash
curl -s --data-urlencode 'data=[out:json][timeout:25];
node(40.75,-74.00,40.77,-73.98)[amenity=restaurant];
out body 30;' https://overpass-api.de/api/interpreter
```
(Overpass bbox order is `south,west,north,east`.)

## Working with a raw location pin
If the user hands you a bare `latitude`/`longitude` pair (e.g. from a
shared-location message), skip geocoding and pass them straight into
`nearby` or `distance`.

## Workflow examples
**"Italian restaurants near the Colosseum":** `search` "Colosseum Rome" →
feed lat/lon into an Overpass `around` query with
`[amenity=restaurant][cuisine=italian]`.

**"Walk from hotel to conference center":** `search` both names → OSRM
`walking` route with `steps=true`.

**"What's open near this pin?":** Overpass `nearby` query on the given
lat/lon, then `web_search` to confirm hours if the tag is missing.

## Pitfalls
- Nominatim: 1 req/s, `User-Agent` header required, or requests get dropped
- Overpass can be slow at peak hours — fall back to the mirror above
- OSRM coordinate order is `lon,lat`; Nominatim/Overpass use `lat,lon` — easy
  to swap by mistake, double check before sending
- A bare zip/postal code can be globally ambiguous — add country/state to
  the query
- `distance`/`directions` need coordinates for both endpoints — geocode
  place names before routing, OSRM doesn't accept addresses

*Adapted from Hermes Agent (MIT, © 2025 Nous Research).*
