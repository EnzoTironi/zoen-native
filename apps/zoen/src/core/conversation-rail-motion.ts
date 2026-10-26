/** A vertical pan must fail early on horizontal movement so cards keep their own scroll. */
export const CONVERSATION_RAIL_GRIP = 28;
export const CONVERSATION_RAIL_PAN = Object.freeze({
  activeOffsetY: [-10, 10] as [number, number],
  failOffsetX: [-12, 12] as [number, number],
  maxPointers: 1,
});

export function railFraction(value: number): number {
  'worklet';
  return Number.isFinite(value) ? Math.min(1, Math.max(0, value)) : 0;
}

export function railVisibleHeight(
  contentHeight: number,
  expansion: number,
  keyboard: number,
  collapsible: boolean,
): number {
  'worklet';
  if (!Number.isFinite(contentHeight) || contentHeight <= 0) return 0;
  const resting = collapsible
    ? CONVERSATION_RAIL_GRIP + contentHeight * railFraction(expansion)
    : contentHeight;
  return resting * (1 - railFraction(keyboard));
}

export function railDragExpansion(start: number, translation: number, height: number): number {
  'worklet';
  if (!Number.isFinite(translation) || !Number.isFinite(height) || height <= 0)
    return railFraction(start);
  return railFraction(start + translation / height);
}

export function railSnapExpansion(expansion: number, velocity: number, height: number): 0 | 1 {
  'worklet';
  // Project a short part of the finger's motion, then snap to one of two detents.
  const projected = railDragExpansion(
    expansion,
    Number.isFinite(velocity) ? velocity * 0.15 : 0,
    height,
  );
  return projected >= 0.5 ? 1 : 0;
}
