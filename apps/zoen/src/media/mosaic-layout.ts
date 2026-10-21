export type MosaicFrame = Readonly<{
  x: number;
  y: number;
  width: number;
  height: number;
  position: number;
}>;
export type MosaicLayout = Readonly<{
  width: number;
  height: number;
  items: readonly MosaicFrame[];
}>;
export const supportsMediaMosaic = false;
/** Other platforms retain canonical individual rows until their layout adapter
 * is qualified. They never receive an invented substitute album algorithm. */
export function mediaMosaicLayout(
  _width: number,
  _scale: number,
  _sizes: readonly (readonly [number, number])[],
): MosaicLayout | null {
  return null;
}
