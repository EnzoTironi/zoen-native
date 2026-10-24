import { nativeLocationPoint } from './native-location.ts';
import type { StaticLocation } from './types.ts';

export const LOCATION_FILE = { name: 'location.geojson', mime: 'application/geo+json' } as const;
export function isLocationFileMime(mime: string): boolean {
  return mime.split(';', 1)[0]?.trim().toLowerCase() === LOCATION_FILE.mime;
}
export function locationGeoJSON(input: StaticLocation): string {
  const point = nativeLocationPoint(input);
  return JSON.stringify({
    type: 'Feature',
    geometry: { type: 'Point', coordinates: [point.longitude, point.latitude] },
    properties: point.label === undefined ? {} : { label: point.label },
  });
}
function record(value: unknown, keys: readonly string[]): Record<string, unknown> {
  if (
    !value ||
    typeof value !== 'object' ||
    Array.isArray(value) ||
    Object.keys(value).some((key) => !keys.includes(key))
  )
    throw Error('location-invalid');
  return value as Record<string, unknown>;
}
/** Used only for our one-point attachment format, never arbitrary GeoJSON collections. */
export function parseLocationGeoJSON(text: string): StaticLocation {
  if (new TextEncoder().encode(text).length > 4096) throw Error('location-invalid');
  const feature = record(JSON.parse(text), ['type', 'geometry', 'properties']);
  const geometry = record(feature.geometry, ['type', 'coordinates']);
  const properties = record(feature.properties, ['label']);
  const coordinates = geometry.coordinates;
  if (
    feature.type !== 'Feature' ||
    geometry.type !== 'Point' ||
    !Array.isArray(coordinates) ||
    coordinates.length !== 2
  )
    throw Error('location-invalid');
  return nativeLocationPoint({
    longitude: coordinates[0],
    latitude: coordinates[1],
    ...properties,
  });
}
