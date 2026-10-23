import { t } from '../i18n/index.ts';
import type { LocalMediaFile, MediaDriver } from './types';

/** Reuse the canonical loader; reject/release late or substituted files before presentation. */
export async function materializeObservedFile(
  media: MediaDriver,
  source: { itemId: string; version?: number; expectedHash?: string },
  current: () => boolean,
): Promise<LocalMediaFile> {
  if (!current()) throw new Error(t('media.thisFileCouldNotBeOpened'));
  const file = await media.materializeFile(source.itemId, source.version);
  if (!file) throw new Error(t('media.thisFileIsStillArrivingTryAgainWhen'));
  if (
    !current() ||
    file.itemId !== source.itemId ||
    (source.version !== undefined && file.version !== source.version) ||
    (source.expectedHash && file.info.sha256 !== source.expectedHash)
  ) {
    await media.releaseLocalFile(file.uri);
    throw new Error(t('media.thisFileCouldNotBeOpened'));
  }
  return file;
}
