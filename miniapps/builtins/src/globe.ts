/** Orthographic globe math ported from apple/Shared/Apps/Native/MapTap.swift.
 * Coordinates are degrees; projected x/y/z use a unit sphere (z < 0 is hidden).
 */
export type GeoPoint = { lat: number; lon: number };
export const GLOBE_SIDE = 360;
export const GLOBE_RADIUS = GLOBE_SIDE * 0.47;
const radians = Math.PI / 180;
const degrees = 180 / Math.PI;
const clamp = (value: number, low: number, high: number) => Math.max(low, Math.min(high, value));

export function normalizeLongitude(value: number): number {
  return ((((value + 180) % 360) + 360) % 360) - 180;
}

export function project(point: GeoPoint, center: GeoPoint) {
  const lat = point.lat * radians;
  const lon = (point.lon - center.lon) * radians;
  const latitude = center.lat * radians;
  return {
    x: Math.cos(lat) * Math.sin(lon),
    y: Math.cos(latitude) * Math.sin(lat) - Math.sin(latitude) * Math.cos(lat) * Math.cos(lon),
    z: Math.sin(latitude) * Math.sin(lat) + Math.cos(latitude) * Math.cos(lat) * Math.cos(lon),
  };
}

export function inverse(x: number, y: number, center: GeoPoint): GeoPoint | null {
  const rho = Math.hypot(x, y);
  if (!Number.isFinite(rho) || rho > 1) return null;
  if (rho < 1e-9) return { ...center };
  const distance = Math.asin(rho);
  const latitude = center.lat * radians;
  const lat = Math.asin(
    clamp(
      Math.cos(distance) * Math.sin(latitude) + (y * Math.sin(distance) * Math.cos(latitude)) / rho,
      -1,
      1,
    ),
  );
  const lon =
    center.lon * radians +
    Math.atan2(
      x * Math.sin(distance),
      rho * Math.cos(distance) * Math.cos(latitude) - y * Math.sin(distance) * Math.sin(latitude),
    );
  return { lat: lat * degrees, lon: normalizeLongitude(lon * degrees) };
}

export function rotate(center: GeoPoint, dx: number, dy: number, side: number): GeoPoint {
  if (!Number.isFinite(side) || side <= 0) return { ...center };
  return {
    lat: clamp(center.lat + (dy / side) * 140, -80, 80),
    lon: normalizeLongitude(center.lon - (dx / side) * 160),
  };
}

function vector(point: GeoPoint): [number, number, number] {
  const lat = point.lat * radians;
  const lon = point.lon * radians;
  return [Math.cos(lat) * Math.cos(lon), Math.cos(lat) * Math.sin(lon), Math.sin(lat)];
}

export function slerp(a: GeoPoint, b: GeoPoint, t: number): GeoPoint {
  if (t <= 0) return { ...a };
  if (t >= 1) return { ...b };
  const av = vector(a),
    bv = vector(b);
  const dot = clamp(av[0] * bv[0] + av[1] * bv[1] + av[2] * bv[2], -1, 1);
  const angle = Math.acos(dot);
  if (angle < 1e-6) return { ...a };
  let x: number, y: number, z: number;
  if (Math.abs(Math.sin(angle)) < 1e-8) {
    // An antipodal pair has no unique shortest arc. Pick a stable perpendicular
    // direction instead of dividing by zero and producing invalid SVG paths.
    const axis: [number, number, number] = Math.abs(av[0]) < 0.8 ? [1, 0, 0] : [0, 1, 0];
    const perpendicular = [
      av[1] * axis[2] - av[2] * axis[1],
      av[2] * axis[0] - av[0] * axis[2],
      av[0] * axis[1] - av[1] * axis[0],
    ];
    const length = Math.hypot(...perpendicular);
    const first = Math.cos(Math.PI * t),
      second = Math.sin(Math.PI * t) / length;
    x = first * av[0] + second * perpendicular[0];
    y = first * av[1] + second * perpendicular[1];
    z = first * av[2] + second * perpendicular[2];
  } else {
    const first = Math.sin((1 - t) * angle) / Math.sin(angle);
    const second = Math.sin(t * angle) / Math.sin(angle);
    x = first * av[0] + second * bv[0];
    y = first * av[1] + second * bv[1];
    z = first * av[2] + second * bv[2];
  }
  return { lat: Math.atan2(z, Math.hypot(x, y)) * degrees, lon: Math.atan2(y, x) * degrees };
}

export function canvasPoint(point: GeoPoint, center: GeoPoint, rim = false) {
  let { x, y, z } = project(point, center);
  if (rim && z < 0) {
    const length = Math.max(1e-9, Math.hypot(x, y));
    x /= length;
    y /= length;
  }
  return { x: GLOBE_SIDE / 2 + GLOBE_RADIUS * x, y: GLOBE_SIDE / 2 - GLOBE_RADIUS * y, z };
}

const coordinates = (point: { x: number; y: number }) =>
  `${point.x.toFixed(3)},${point.y.toFixed(3)}`;

/** The same horizon/rim projection as the existing native land renderer. */
export function landPath(polygons: readonly (readonly number[])[], center: GeoPoint): string {
  let path = '';
  for (const polygon of polygons) {
    let part = '',
      visible = false;
    for (let index = 0; index + 1 < polygon.length; index += 2) {
      const point = canvasPoint({ lon: polygon[index], lat: polygon[index + 1] }, center, true);
      visible ||= point.z > 0;
      part += `${index === 0 ? 'M' : 'L'}${coordinates(point)}`;
    }
    if (visible) path += `${part}Z`;
  }
  return path;
}

export function visiblePath(points: readonly GeoPoint[], center: GeoPoint): string {
  let path = '',
    started = false;
  for (const point of points) {
    const projected = canvasPoint(point, center);
    if (projected.z < 0) {
      started = false;
      continue;
    }
    path += `${started ? 'L' : 'M'}${coordinates(projected)}`;
    started = true;
  }
  return path;
}

export function graticulePath(center: GeoPoint): string {
  let path = '';
  for (let lat = -60; lat <= 60; lat += 30) {
    const points: GeoPoint[] = [];
    for (let lon = -180; lon <= 180; lon += 6) points.push({ lat, lon });
    path += visiblePath(points, center);
  }
  for (let lon = -180; lon < 180; lon += 30) {
    const points: GeoPoint[] = [];
    for (let lat = -90; lat <= 90; lat += 6) points.push({ lat, lon });
    path += visiblePath(points, center);
  }
  return path;
}

export function arcPath(a: GeoPoint, b: GeoPoint, center: GeoPoint): string {
  return visiblePath(
    Array.from({ length: 65 }, (_, index) => slerp(a, b, index / 64)),
    center,
  );
}
