import type { AccountDto } from '../core/generated/AccountDto';
import type { MediaPlaybackScope } from '../core/generated/MediaPlaybackScope';
import type { FileAttachmentProps } from './file-attachment-types';
import type { LocalMediaFile } from './types';

/** A playback hint from the existing lease; Core still validates the actual message and bytes. */
export function messagePlaybackScope(
  mode: string,
  account: AccountDto | null | undefined,
  source: FileAttachmentProps['messageSource'],
  file: LocalMediaFile,
): MediaPlaybackScope | undefined {
  if (process.env.EXPO_OS !== 'ios' || mode !== 'account' || !account || !source) return undefined;
  return {
    expected_identity_id: account.identity_id,
    expected_device_id: account.device_id,
    space_id: source.spaceId,
    message_id: source.entry.id,
    item_id: file.itemId,
    version: file.version,
    sha256: file.info.sha256,
  };
}
