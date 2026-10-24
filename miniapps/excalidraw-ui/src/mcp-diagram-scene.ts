import { convertToExcalidrawElements, FONT_FAMILY } from "@excalidraw/excalidraw";

export function parsePartialElements(str: string | undefined): any[] {
  if (!str?.trim().startsWith("[")) return [];
  try { return JSON.parse(str); } catch { /* partial */ }
  const last = str.lastIndexOf("}");
  if (last < 0) return [];
  try { return JSON.parse(str.substring(0, last + 1) + "]"); } catch { /* incomplete */ }
  return [];
}

export function excludeIncompleteLastItem<T>(arr: T[]): T[] {
  if (!arr || arr.length === 0) return [];
  if (arr.length <= 1) return [];
  return arr.slice(0, -1);
}

export interface ViewportRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Convert raw shorthand elements → Excalidraw format (labels → bound text, font fix).
 *  Preserves pseudo-elements like cameraUpdate (not valid Excalidraw types). */
export function convertRawElements(els: any[]): any[] {
  const pseudoTypes = new Set(["cameraUpdate", "delete", "restoreCheckpoint"]);
  const pseudos = els.filter((el: any) => pseudoTypes.has(el.type));
  const real = els.filter((el: any) => !pseudoTypes.has(el.type));
  const withDefaults = real.map((el: any) =>
    el.label ? { ...el, label: { textAlign: "center", verticalAlign: "middle", ...el.label } } : el
  );
  const converted = convertToExcalidrawElements(withDefaults, { regenerateIds: false })
    .map((el: any) => el.type === "text" ? { ...el, fontFamily: (FONT_FAMILY as any).Excalifont ?? 1 } : el);
  return [...converted, ...pseudos];
}

/** Fix SVG viewBox to 4:3 by expanding the smaller dimension and centering. */
export function fixViewBox4x3(svg: SVGSVGElement): void {
  const vb = svg.getAttribute("viewBox")?.split(" ").map(Number);
  if (!vb || vb.length !== 4) return;
  const [vx, vy, vw, vh] = vb;
  const r = vw / vh;
  if (Math.abs(r - 4 / 3) < 0.01) return;
  if (r > 4 / 3) {
    const h2 = Math.round(vw * 3 / 4);
    svg.setAttribute("viewBox", `${vx} ${vy - Math.round((h2 - vh) / 2)} ${vw} ${h2}`);
  } else {
    const w2 = Math.round(vh * 4 / 3);
    svg.setAttribute("viewBox", `${vx - Math.round((w2 - vw) / 2)} ${vy} ${w2} ${vh}`);
  }
}

export function extractViewportAndElements(elements: any[]): {
  viewport: ViewportRect | null;
  drawElements: any[];
  restoreId: string | null;
  deleteIds: Set<string>;
} {
  let viewport: ViewportRect | null = null;
  let restoreId: string | null = null;
  const deleteIds = new Set<string>();
  const drawElements: any[] = [];

  for (const el of elements) {
    if (el.type === "cameraUpdate") {
      viewport = { x: el.x, y: el.y, width: el.width, height: el.height };
    } else if (el.type === "restoreCheckpoint") {
      restoreId = el.id;
    } else if (el.type === "delete") {
      for (const id of String(el.ids ?? el.id).split(",")) deleteIds.add(id.trim());
    } else {
      drawElements.push(el);
    }
  }

  // Hide deleted elements via near-zero opacity instead of removing — preserves SVG
  // group count/order so morphdom matches by position correctly (no cascade re-animations).
  // Using 1 (not 0) because Excalidraw treats opacity:0 as "unset" → defaults to 100.
  const processedDraw = deleteIds.size > 0
    ? drawElements.map((el: any) => (deleteIds.has(el.id) || deleteIds.has(el.containerId)) ? { ...el, opacity: 1 } : el)
    : drawElements;

  return { viewport, drawElements: processedDraw, restoreId, deleteIds };
}

export const EXPORT_PADDING = 20;

/**
 * Compute the min x/y of all draw elements in scene coordinates.
 * This matches the offset Excalidraw's exportToSvg applies internally:
 *   SVG_x = scene_x - sceneMinX + exportPadding
 */
export function computeSceneBounds(elements: any[]): { minX: number; minY: number } {
  let minX = Infinity;
  let minY = Infinity;
  for (const el of elements) {
    if (el.x != null) {
      minX = Math.min(minX, el.x);
      minY = Math.min(minY, el.y);
      // Arrow points are offsets from el.x/y
      if (el.points && Array.isArray(el.points)) {
        for (const pt of el.points) {
          minX = Math.min(minX, el.x + pt[0]);
          minY = Math.min(minY, el.y + pt[1]);
        }
      }
    }
  }
  return { minX: isFinite(minX) ? minX : 0, minY: isFinite(minY) ? minY : 0 };
}

/**
 * Convert a scene-space viewport rect to an SVG-space viewBox.
 */
export function sceneToSvgViewBox(
  vp: ViewportRect,
  sceneMinX: number,
  sceneMinY: number,
): { x: number; y: number; w: number; h: number } {
  return {
    x: vp.x - sceneMinX + EXPORT_PADDING,
    y: vp.y - sceneMinY + EXPORT_PADDING,
    w: vp.width,
    h: vp.height,
  };
}

