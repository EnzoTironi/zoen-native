// SPDX-License-Identifier: GPL-2.0-or-later
// Telegram-iOS f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838:
// ChatTextInputPanelNode:3260-3293,3407-3466,4571-4776,4793;
// ChatTextInputActionButtonsNode:234-251; CAAnimationUtils spring fallback.
import { telegramComposer } from './telegram-composer-presentation.ts';

export const telegramComposerMotion = Object.freeze({
  frameDuration: 400,
  frameCurve: [0.38, 0.7, 0.125, 1] as const,
  alphaDuration: 200,
  alphaCurve: [0.42, 0, 0.58, 1] as const,
  accessoryDuration: 250,
  sendHiddenScale: 0.001,
  accessoryHiddenScale: 0.2,
  sendHiddenOffset: (telegramComposer.sendSlot - telegramComposer.sendInset * 2) / 2 - 3,
  sendImageOffset: { x: -22, y: 18 },
  // The original media frame is width+8. Our row already has the outer8pt inset.
  mediaHiddenOffset: telegramComposer.externalControl + telegramComposer.edge * 2,
  sendHighlightedScale: 0.75,
  sendHighlightDuration: 400,
  sendHighlightRecoveryDuration: 250,
});

export type ComposerMotionRole = 'send' | 'media' | 'accessory' | 'search';

/** Hit testing follows the target immediately, independently of presentation alpha. */
export function composerMotionTarget(
  role: ComposerMotionRole,
  active: boolean,
  hasSend = false,
  visible = active,
) {
  return {
    opacity: visible ? 1 : 0,
    scale:
      role === 'send'
        ? active
          ? 1
          : telegramComposerMotion.sendHiddenScale
        : role === 'accessory' && !active
          ? telegramComposerMotion.accessoryHiddenScale
          : 1,
    x:
      role === 'send'
        ? active
          ? 0
          : telegramComposerMotion.sendHiddenOffset
        : role === 'media'
          ? active
            ? 0
            : telegramComposerMotion.mediaHiddenOffset
          : role === 'accessory' && hasSend
            ? -telegramComposer.sendSlot
            : 0,
  };
}

export function composerExternalWidth({
  searching,
  hasSend,
  hasVoice,
}: {
  searching: boolean;
  hasSend: boolean;
  hasVoice: boolean;
}) {
  if (searching && !hasSend) return 44 * 2 + 2 + telegramComposer.gap;
  return !searching && !hasSend && hasVoice
    ? telegramComposer.externalControl + telegramComposer.gap
    : 0;
}
