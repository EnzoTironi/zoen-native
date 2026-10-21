/** Geometry observed at Telegram-iOS f1dd7a2, normal glass chat navigation.
 * The60pt containing bar is distinct from its44pt controls. These are owned
 * control frames, never estimates of UIKit's private title/control layout.
 */
export function chatHeaderLayout({
  width,
  top,
  left,
  right,
  back,
  search,
}: {
  width: number;
  top: number;
  left: number;
  right: number;
  back: boolean;
  search: boolean;
}) {
  const leading = left + 16;
  const trailingWidth = search ? 88 : 44;
  const trailing = width - right - 16 - trailingWidth;
  const rightTitleInset = right + 16 + trailingWidth + 10;
  // NavigationBarImpl1072–1079 balances a missing leading group.
  const leftTitleInset = back ? leading + 44 + 10 : rightTitleInset;
  const availableWidth = Math.max(0, width - leftTitleInset - rightTitleInset);
  return {
    height: top + 60,
    controlTop: top + 10, // glass2 + (nominal60 − control44)/2
    leading,
    trailing,
    trailingWidth,
    leftTitleInset,
    rightTitleInset,
    availableWidth,
  };
}

/** Center first, then respect the actual groups, as NavigationBarImpl1103–1111.
 * measuredWidth comes only from this title's rendered bounded Pressable.
 */
export function chatHeaderTitlePlacement(
  width: number,
  layout: ReturnType<typeof chatHeaderLayout>,
  measuredWidth: number,
  scale: number,
) {
  const titleWidth = Math.max(0, Math.min(layout.availableWidth, measuredWidth));
  const density = Number.isFinite(scale) && scale > 0 ? scale : 1;
  const floorPixel = (value: number) => Math.floor(value * density) / density;
  let x = floorPixel((width - titleWidth) / 2);
  if (x + titleWidth > width - layout.rightTitleInset) {
    x = width - layout.rightTitleInset - titleWidth;
  }
  if (x < layout.leftTitleInset) {
    x = layout.leftTitleInset + floorPixel((layout.availableWidth - titleWidth) / 2);
  }
  return { x, width: titleWidth };
}
