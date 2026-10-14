export type Point = { x: number; y: number };
export type Size = { width: number; height: number };
export type Camera = { x: number; y: number; zoom: number };
export const initialCamera: Camera = { x: 0, y: 0, zoom: 1 };

/** The same north-up Mercator route used by the online map, fitted above the cards. */
export function fitRoute(line: [number, number][], size: Size): Point[] {
  const points = line.map(([lon, lat]) => ({
    x: lon * Math.PI / 180,
    y: -Math.log(Math.tan(Math.PI / 4 + Math.max(-85.051, Math.min(85.051, lat)) * Math.PI / 360)),
  }));
  if (!points.length) return [];
  const xs = points.map(p => p.x), ys = points.map(p => p.y);
  const left = Math.min(...xs), right = Math.max(...xs), top = Math.min(...ys), bottom = Math.max(...ys);
  const upperInset = Math.min(150, size.height * .18), lowerInset = Math.min(300, size.height * .33);
  const height = Math.max(1, size.height - upperInset - lowerInset);
  const scale = Math.min(Math.max(1, size.width - 100) / Math.max(1e-8, right - left), height / Math.max(1e-8, bottom - top));
  return points.map(p => ({ x: size.width / 2 + (p.x - (left + right) / 2) * scale, y: upperInset + height / 2 + (p.y - (top + bottom) / 2) * scale }));
}

/** Keep the point under a moving finger or pinch centre fixed while panning/zooming. */
export function moveCamera(camera: Camera, size: Size, before: Point, after: Point, factor = 1): Camera {
  const zoom = Math.max(.5, Math.min(8, camera.zoom * factor));
  const ratio = zoom / camera.zoom;
  return {
    zoom,
    x: after.x - size.width / 2 - ratio * (before.x - size.width / 2 - camera.x),
    y: after.y - size.height / 2 - ratio * (before.y - size.height / 2 - camera.y),
  };
}

export function gesture(points: Point[]): { centre: Point; distance: number } | null {
  if (!points.length) return null;
  const pair = points.slice(0, 2);
  const centre = { x: pair.reduce((n, p) => n + p.x, 0) / pair.length, y: pair.reduce((n, p) => n + p.y, 0) / pair.length };
  const distance = pair.length === 2 ? Math.max(1, Math.hypot(pair[1].x - pair[0].x, pair[1].y - pair[0].y)) : 1;
  return { centre, distance };
}
