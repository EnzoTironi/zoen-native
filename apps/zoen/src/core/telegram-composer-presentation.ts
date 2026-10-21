// SPDX-License-Identifier: GPL-2.0-or-later
// Translated Telegram-iOS composer contracts at f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838.
// ChatTextInputPanelNode, ChatTextInputFontMetrics, ChatTextInputActionButtonsNode,
// AccessoryItemIconButton and GlassBackgroundComponent; GPLv3 option selected.
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

/** iOS26 effect parameters only; legacy fallback uses existing shared theme surfaces. */
export function composerGlassAppearance(dark: boolean, preference: ComposerGlassPreference) {
  return {
    style: preference === 'clear' ? ('clear' as const) : ('regular' as const),
    tintColor:
      preference === 'clear'
        ? dark
          ? 'rgba(0, 0, 0, 0.28)'
          : undefined
        : dark
          ? 'rgba(255, 255, 255, 0.025)'
          : 'rgba(255, 255, 255, 0.1)',
    isInteractive: true,
    spacing: telegramComposer.glassSpacing,
  };
}
