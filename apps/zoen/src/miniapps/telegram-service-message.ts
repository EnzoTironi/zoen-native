/** Translated from Telegram-iOS f1dd7a2, GPL-2.0-or-later (GPLv3 selected).
 * ChatMessageActionBubbleContentNode lines429–463 and LinkHighlightingNode's
 * legacy clear/ellipse mask. Attribution and exact hashes live in
 * docs/product/telegram-ios-app-message-sources.json and THIRD_PARTY_NOTICES.md.
 * Only measured text geometry enters this module; it owns no message state. */
export type ServiceLine = Readonly<{ x: number; y: number; width: number; height: number }>;

export function telegramServiceLineRects(lines: readonly ServiceLine[], labelWidth: number) {
  if (
    !Number.isFinite(labelWidth) ||
    labelWidth <= 0 ||
    !lines.length ||
    lines.length > 512 ||
    lines.some((line) =>
      [line.x, line.y, line.width, line.height].some((value) => !Number.isFinite(value)),
    ) ||
    lines.some((line) => line.width < 0 || line.height <= 0)
  )
    return [];
  const rects = lines.map((line) => ({ ...line }));
  const sorted = rects.map((_, i) => i).sort((a, b) => rects[b]!.width - rects[a]!.width);
  for (const index of sorted)
    for (const delta of [-1, 1]) {
      const neighbor = rects[index + delta];
      const rect = rects[index]!;
      if (neighbor && Math.abs(neighbor.width - rect.width) < 40)
        neighbor.width = rect.width = Math.max(neighbor.width, rect.width);
    }
  return rects.map((rect) => {
    // Upstream is22pt at default size; retain larger native text without clipping.
    const height = Math.max(22, Math.ceil(rect.height));
    const width = rect.width + 14;
    return {
      x: Math.floor((labelWidth - width) / 2),
      y: rect.y + Math.floor((rect.height - height) / 2),
      width,
      height,
    };
  });
}

type Corner = 'topLeft' | 'topRight' | 'bottomLeft' | 'bottomRight';
const maxX = (r: ServiceLine) => r.x + r.width;
const maxY = (r: ServiceLine) => r.y + r.height;
const contains = (r: ServiceLine, x: number, y: number) =>
  x >= r.x && x < maxX(r) && y >= r.y && y < maxY(r);

/** One alpha mask, then one fill: overlapping lines never darken the sky twice.
 * SVG encodes the upstream CGContext .copy operations, including concave joins.
 * No user text/URL/markup is interpolated into the image. */
export function telegramServiceMask(rects: readonly ServiceLine[], color: string) {
  if (
    !rects.length ||
    !/^#[\da-f]{6}(?:[\da-f]{2})?$/i.test(color) ||
    rects.some((r) => [r.x, r.y, r.width, r.height].some((value) => !Number.isFinite(value))) ||
    rects.some((r) => r.width <= 0 || r.height <= 0)
  )
    return null;
  const left = Math.min(...rects.map((r) => r.x));
  const top = Math.min(...rects.map((r) => r.y));
  const width = Math.max(...rects.map(maxX)) - left;
  const height = Math.max(...rects.map(maxY)) - top;
  const local = rects.map((r) => ({ ...r, x: r.x - left, y: r.y - top }));
  const mask: string[] = [];
  const rect = (x: number, y: number, w: number, h: number, fill: string) =>
    mask.push(`<rect x="${x}" y="${y}" width="${w}" height="${h}" fill="${fill}"/>`);
  const ellipse = (x: number, y: number, radius: number, fill: string) =>
    mask.push(`<circle cx="${x + radius}" cy="${y + radius}" r="${radius}" fill="${fill}"/>`);
  function full(x: number, y: number, corner: Corner) {
    const r = 11;
    const right = corner.endsWith('Right');
    const bottom = corner.startsWith('bottom');
    rect(x - (right ? r : 0), y - (bottom ? r : 0), r, r, 'black');
    ellipse(x - (right ? 2 * r : 0), y - (bottom ? 2 * r : 0), r, 'white');
  }
  function connecting(x: number, y: number, corner: Corner, radius: number) {
    if (radius <= 0) return;
    const leftCorner = corner.endsWith('Left');
    const bottom = corner.startsWith('bottom');
    rect(x - (leftCorner ? radius : 0), y - (bottom ? radius : 0), radius, radius, 'white');
    ellipse(x - (leftCorner ? 2 * radius : 0), y - (bottom ? 2 * radius : 0), radius, 'black');
  }
  for (const r of local) rect(r.x, r.y, r.width, r.height, 'white');
  for (let i = 0; i < local.length; i++) {
    const r = local[i]!;
    const previous = local[i - 1];
    const next = local[i + 1];
    if (previous) {
      const radius = next ? Math.min(11, Math.floor((next.y - maxY(previous)) / 2)) : 11;
      if (contains(previous, r.x, r.y)) {
        if (Math.abs(r.x - previous.x) >= 11) connecting(r.x, maxY(previous), 'topLeft', radius);
      } else full(r.x, r.y, 'topLeft');
      if (contains(previous, maxX(r) - 1, r.y)) {
        if (Math.abs(maxX(r) - maxX(previous)) >= 11)
          connecting(maxX(r), maxY(previous), 'topRight', radius);
      } else full(maxX(r), r.y, 'topRight');
    } else {
      full(r.x, r.y, 'topLeft');
      full(maxX(r), r.y, 'topRight');
    }
    if (next) {
      const radius = previous ? Math.min(11, Math.floor((next.y - maxY(previous)) / 2)) : 11;
      if (contains(next, r.x, maxY(r))) {
        if (Math.abs(maxX(r) - maxX(next)) >= 11) connecting(r.x, next.y, 'bottomLeft', radius);
      } else full(r.x, maxY(r), 'bottomLeft');
      if (contains(next, maxX(r) - 1, maxY(r))) {
        if (Math.abs(maxX(r) - maxX(next)) >= 11)
          connecting(maxX(r), next.y, 'bottomRight', radius);
      } else full(maxX(r), maxY(r), 'bottomRight');
    } else {
      full(r.x, maxY(r), 'bottomLeft');
      full(maxX(r), maxY(r), 'bottomRight');
    }
  }
  const fill = color.slice(0, 7);
  const opacity = color.length === 9 ? parseInt(color.slice(7), 16) / 255 : 1;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}"><defs><mask id="m" maskUnits="userSpaceOnUse" x="0" y="0" width="${width}" height="${height}" style="mask-type:luminance">${mask.join('')}</mask></defs><rect width="${width}" height="${height}" fill="${fill}" fill-opacity="${opacity}" mask="url(#m)"/></svg>`;
  return { x: left, y: top, width, height, uri: `data:image/svg+xml,${encodeURIComponent(svg)}` };
}
