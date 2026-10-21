import type { MediaDraftAttachmentOptions } from './media-draft-types';

export type PhotoSendMode = 'default' | 'send-as-file' | 'send-without-grouping';

/** Existing draft options are an admitted batch contract, not another picker
 * preference. A fresh multi-selection follows Telegram's grouped default. */
export function photoSendOptions(
  count: number,
  mode: PhotoSendMode,
  existing?: MediaDraftAttachmentOptions,
): MediaDraftAttachmentOptions | undefined {
  if (!Number.isInteger(count) || count < 1 || count > 10)
    throw new Error('Invalid media selection');
  if (mode === 'send-as-file') {
    if (existing && existing.presentation !== 'file') throw new Error('The media draft changed');
    return { presentation: 'file', grouping: 'separate' };
  }
  if (mode === 'send-without-grouping') {
    if (
      count < 2 ||
      (existing && (existing.presentation !== 'media' || existing.grouping !== 'separate'))
    )
      throw new Error('The media draft changed');
    return { presentation: 'media', grouping: 'separate' };
  }
  if (existing) return { ...existing };
  return count > 1 ? { presentation: 'media', grouping: 'grouped' } : undefined;
}
