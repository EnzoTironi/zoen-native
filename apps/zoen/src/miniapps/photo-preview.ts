import { t } from '../i18n/index.ts';
import { MAX_PHOTO_DATA_URL, type CapabilityLease, type PhotoPreview } from './capability-data';

/** Decode downsampled in SDK58, then encode fresh pixels: source EXIF/GPS is discarded. */
export async function jpegPreview(
  uri: string,
  maxSide: number,
  lease: CapabilityLease,
  release: (uri: string) => void,
): Promise<Omit<PhotoPreview, 'token'>> {
  const { ImageManipulator, SaveFormat } = await import('expo-image-manipulator');
  lease.check();
  let side = maxSide;
  for (let attempt = 0; attempt < 5; attempt++) {
    const context = ImageManipulator.manipulate(uri, { maxWidth: side, maxHeight: side });
    let image: Awaited<ReturnType<typeof context.renderAsync>> | undefined;
    let savedUri: string | undefined;
    try {
      image = await context.renderAsync();
      lease.check();
      const result = await image.saveAsync({
        format: SaveFormat.JPEG,
        compress: 0.62,
        base64: true,
      });
      savedUri = result.uri;
      lease.check();
      const dataUrl = `data:image/jpeg;base64,${result.base64 ?? ''}`;
      if (result.base64 && dataUrl.length <= MAX_PHOTO_DATA_URL)
        return { dataUrl, width: result.width, height: result.height, locationStripped: true };
    } finally {
      image?.release();
      context.release();
      if (savedUri) release(savedUri);
    }
    side = Math.max(1, Math.floor(side * 0.75));
  }
  throw new Error(t('miniapps.thisPhotoCouldNotBeResizedChooseAnother'));
}
