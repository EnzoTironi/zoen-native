import { File } from 'expo-file-system';
import { t } from '../i18n/index.ts';
import type { LocalMediaFile, MediaDriver } from './types';
import { mediaPlayback } from './playback-owner';

/** Own one preview lease until the native viewer has actually finished with it. */
export async function previewMediaFile(media: MediaDriver, file: LocalMediaFile): Promise<void> {
  try {
    mediaPlayback.pauseAll();
    if (media.previewFile) {
      await media.previewFile(file);
      return;
    }
    const preview = new File(file.uri);
    const canPreview = await preview.canPreview({ mimeType: file.info.mime });
    if (!canPreview) throw new Error(t('media.thisFormatCannotBePreviewedOnThisDevice'));
    await preview.preview({ title: file.info.name, mimeType: file.info.mime });
  } finally {
    await media.releaseLocalFile(file.uri);
  }
}
