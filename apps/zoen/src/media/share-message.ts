import * as Sharing from 'expo-sharing';
import type { TimelineEntry } from '../core/types';
import { t } from '../i18n/index.ts';
import type { MediaDriver } from './types';

/** One exact signed attachment; the share sheet owns its temporary copy until dismissal. */
export async function shareMessageMedia(
  entry: TimelineEntry,
  media: MediaDriver,
  isCurrent: () => boolean = () => true,
): Promise<void> {
  if (!isCurrent()) return;
  if (entry.deleted) throw new Error(t('media.thisFileCouldNotBeOpened'));
  const card = 'Message' in entry.kind ? entry.kind.Message.card : null;
  const item = entry.voice?.item_id ?? (card?.kind_id === 'file' ? card.item_id : null);
  if (!item) throw new Error(t('media.thisFileCouldNotBeOpened'));
  const available = await Sharing.isAvailableAsync();
  if (!isCurrent()) return;
  if (!available) throw new Error(t('media.sharingIsNotAvailableOnThisDevice'));
  const local = await media.materializeFile(item, entry.voice ? undefined : card?.version);
  if (!local) throw new Error(t('media.thisFileIsStillArrivingTryAgainWhen'));
  try {
    if (!isCurrent()) return;
    if (entry.voice && local.info.sha256 !== entry.voice.sha256)
      throw new Error(t('media.theAudioHasBeenUpdatedOpenTheMessage'));
    await Sharing.shareAsync(local.uri, {
      mimeType: local.info.mime,
      dialogTitle: local.info.name,
    });
  } finally {
    await media.releaseLocalFile(local.uri);
  }
}
