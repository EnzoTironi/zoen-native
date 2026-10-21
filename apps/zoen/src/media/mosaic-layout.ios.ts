import CoreModule from '../../modules/zoen-core/src/ZoenCoreModule';
import type { MosaicLayout } from './mosaic-layout';
export const supportsMediaMosaic = Boolean(CoreModule?.attachmentMosaicLayout);

/** The same original Swift algorithm owns selected previews and chat albums. */
export function mediaMosaicLayout(
  width: number,
  scale: number,
  sizes: readonly (readonly [number, number])[],
): MosaicLayout | null {
  if (!CoreModule?.attachmentMosaicLayout) return null;
  try {
    const result = CoreModule.attachmentMosaicLayout(
      width,
      width,
      scale,
      sizes.map((size) => [...size]),
    );
    if (
      !result ||
      result.items.length !== sizes.length ||
      ![result.width, result.height].every((value) => Number.isFinite(value) && value > 0) ||
      result.items.some(
        (frame) =>
          ![frame.x, frame.y, frame.width, frame.height].every(Number.isFinite) ||
          frame.x < 0 ||
          frame.y < 0 ||
          frame.width <= 0 ||
          frame.height <= 0 ||
          frame.x + frame.width > result.width + 1 ||
          frame.y + frame.height > result.height + 1,
      )
    )
      return null;
    return result;
  } catch {
    return null;
  }
}
