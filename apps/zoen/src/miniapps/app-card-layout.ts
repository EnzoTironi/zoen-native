/** One native cover contract for every App surface; never a web-runtime size. */
export const APP_CARD_METRICS = Object.freeze({
  size: 160,
  radius: 24,
  padding: 12,
  gap: 10,
  contentGap: 6,
  menuSize: 44,
  artworkSize: 52,
  titleFontSize: 14,
  titleLineHeight: 17,
  valueFontSize: 32,
  valueLineHeight: 36,
  dateFontSize: 26,
  dateLineHeight: 32,
  codeFontSize: 24,
  codeLineHeight: 28,
  detailFontSize: 11,
  detailLineHeight: 14,
});

export function appCardGrid(availableWidth: number) {
  const { size, gap } = APP_CARD_METRICS;
  const width = Number.isFinite(availableWidth) ? Math.max(0, availableWidth) : size;
  const columns = Math.max(1, Math.floor((width + gap) / (size + gap)));
  return { columns, width: columns * size + (columns - 1) * gap };
}

/** Keep scaled title first. Remove metadata, then the visual; never shrink fonts.
 * A cover summarizes content; its accessible action retains the complete summary. */
export function appCardContentLayout(fontScale: number, hasAccessory = false) {
  const m = APP_CARD_METRICS;
  const scale = Number.isFinite(fontScale) && fontScale > 0 ? fontScale : 1;
  const inner = m.size - 2 * m.padding;
  const line = Math.ceil(m.titleLineHeight * scale);
  const visualHeight = Math.max(54, Math.ceil(m.valueLineHeight * scale));
  const metadataHeight = Math.ceil(m.detailLineHeight * scale) + 7;
  const twoLineTitle = 2 * line;
  const showVisual = visualHeight + m.contentGap + twoLineTitle <= inner;
  const showMetadata =
    scale <= 1.2 &&
    showVisual &&
    visualHeight + twoLineTitle + metadataHeight + 2 * m.contentGap <= inner;
  const clearance = !showVisual && hasAccessory ? m.menuSize : 0;
  const titleBudget = showVisual ? twoLineTitle : inner - clearance;
  const titleLines = Math.max(0, Math.min(2, Math.floor(titleBudget / line)));
  const titleHeight = titleLines * line;
  return {
    scale,
    showVisual,
    showMetadata,
    visualHeight,
    visualWidth: inner - (hasAccessory ? m.menuSize : 0),
    titleLines,
    titleHeight,
    titleTop: showVisual
      ? m.padding + visualHeight + m.contentGap
      : m.padding + clearance + (inner - clearance - titleHeight) / 2,
    metadataHeight,
    metadataTop: m.size - m.padding - metadataHeight,
  };
}
