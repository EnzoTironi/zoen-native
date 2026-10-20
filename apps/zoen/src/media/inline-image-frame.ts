export type DecodedMessageImage = Readonly<{
  itemId: string;
  version: number;
  sha256: string;
  width: number;
  height: number;
}>;

/** Resolve both dimensions before Yoga; image and mask share this exact rectangle. */
export function inlineImageFrame(availableWidth: number, sourceRatio: number) {
  const available = Number.isFinite(availableWidth) && availableWidth > 0 ? availableWidth : 0;
  const ratio = Number.isFinite(sourceRatio) && sourceRatio > 0 ? sourceRatio : 1;
  const width = Math.min(available, 360 * ratio);
  return { width, height: width / ratio };
}
