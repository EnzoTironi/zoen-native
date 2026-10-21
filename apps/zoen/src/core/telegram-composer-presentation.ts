// SPDX-License-Identifier: GPL-2.0-or-later
// Translated Telegram-iOS composer contracts at f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838.
// ChatTextInputPanelNode, ChatTextInputFontMetrics, ChatTextInputActionButtonsNode,
// AccessoryItemIconButton, GlassBackgroundComponent and WallpaperBackgroundNode; GPLv3 option selected.
import { composerVerticalInsets } from './telegram-audio-presentation.ts';

export const telegramComposer = Object.freeze({
  edge: 8,
  externalControl: 40,
  gap: 6,
  sendSlot: 46,
  sendInset: 3,
  glassSpacing: 7,
  inputModeControl: 32,
  accessoryLeading: 4,
  accessoryMeasurementAdjustment: 2,
  internalTop: 5,
  internalBottom: 4,
  internalLeft: 12,
  internalRight: 11,
  accessoryClearanceAdjustment: 14,
  actionClearanceAdjustment: 10,
});

/** The seven actual source steps. Other Zoen/custom-scaled fonts require native fitting. */
export function composerSourceMinimumHeight(fontSize: number): number | null {
  switch (fontSize) {
    case 14:
    case 15:
    case 16:
    case 17:
      return 40;
    case 19:
      return 42;
    case 23:
      return 47;
    case 26:
      return 51;
    default:
      return null;
  }
}

/** minimumHeight is the validated single-line minimum, not the current multiline height. */
export function composerSendGeometry(minimumHeight = 40) {
  const height = Number.isFinite(minimumHeight) && minimumHeight >= 40 ? minimumHeight : 40;
  const fillHeight = height - telegramComposer.sendInset * 2;
  return {
    fieldRadius: Math.floor(height / 2),
    slotWidth: telegramComposer.sendSlot,
    slotHeight: height,
    fillX: telegramComposer.sendInset,
    fillY: telegramComposer.sendInset,
    fillWidth: telegramComposer.sendSlot - telegramComposer.sendInset * 2,
    fillHeight,
    fillRadius: Math.min(40, fillHeight) / 2,
  };
}

/** Keep the editor frame's insets separate from the inner text-container insets. */
export function composerFieldInsets({
  fontSize,
  accessoryButtonsWidth = 0,
  hasSend,
}: {
  fontSize: number;
  /** Source measurement includes first4pt inset and input-item minus2pt adjustment. */
  accessoryButtonsWidth?: number;
  hasSend: boolean;
}) {
  const vertical = composerVerticalInsets(fontSize);
  const width = Number.isFinite(accessoryButtonsWidth) ? Math.max(0, accessoryButtonsWidth) : 0;
  let right = Math.max(0, width - telegramComposer.accessoryClearanceAdjustment);
  if (hasSend) right += telegramComposer.sendSlot - telegramComposer.actionClearanceAdjustment;
  else right = Math.max(0, right - telegramComposer.actionClearanceAdjustment);
  return {
    internal: { top: 5, bottom: 4, left: 12, right: 11 },
    textContainer: { top: vertical.top - 5, bottom: vertical.bottom - 4, left: 0, right },
    total: { ...vertical, left: 12, right: 11 + right },
  };
}

export type ComposerGlassPreference = 'panel' | 'clear';
/** ChatController: a saturated wallpaper chooses clear only in dark mode; override wins. */
export function composerPreferredGlass({
  dark,
  saturatedWallpaper = false,
  forceClear = false,
}: {
  dark: boolean;
  saturatedWallpaper?: boolean;
  forceClear?: boolean;
}): ComposerGlassPreference {
  return forceClear || (dark && saturatedWallpaper) ? 'clear' : 'panel';
}

/** WallpaperBackgroundNode:55-90 at the same pin; current SkyLayer's gradient
 * colors are supplied by its existing theme owner. No new wallpaper authority. */
export function composerWallpaperSaturation(colors: readonly string[]): number {
  const hsb = colors
    .filter((color) => /^#[\da-f]{6}$/i.test(color))
    .map((color) => {
      const [r, g, b] = [1, 3, 5].map(
        (offset) => parseInt(color.slice(offset, offset + 2), 16) / 255,
      ) as [number, number, number];
      const high = Math.max(r, g, b),
        low = Math.min(r, g, b),
        delta = high - low;
      const hue =
        delta === 0
          ? 0
          : high === r
            ? ((g - b) / delta) % 6
            : high === g
              ? (b - r) / delta + 2
              : (r - g) / delta + 4;
      return { h: (hue / 6 + 1) % 1, s: high === 0 ? 0 : delta / high, b: high };
    });
  const diverse = hsb.some((a, index) =>
    hsb.slice(index + 1).some((b) => {
      const difference = Math.abs(a.h - b.h);
      return Math.min(difference, 1 - difference) > 0.167;
    }),
  );
  return hsb.reduce(
    (maximum, color) =>
      Math.max(
        maximum,
        color.s * (diverse ? Math.min(color.b * 2, 1) : Math.min(color.b, 1 - color.b) * 2),
      ),
    0,
  );
}
