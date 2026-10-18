import { t } from '../i18n';
import type { PhotoDraft } from './types';

export function pickProfilePhoto(): Promise<PhotoDraft | null> {
  // Open synchronously inside the button gesture; browsers require user activation.
  return new Promise((resolve, reject) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = 'image/jpeg,image/png,image/webp,image/heic';
    input.addEventListener('cancel', () => resolve(null), { once: true });
    input.addEventListener(
      'change',
      () => {
        const file = input.files?.[0];
        if (!file) {
          resolve(null);
          return;
        }
        void (async () => {
          if (!file.type.startsWith('image/') || file.size > 20 * 1024 * 1024)
            throw new Error(t('profile.pickerLimit'));
          const image = await createImageBitmap(file, { resizeWidth: 1024, resizeQuality: 'high' });
          try {
            const side = Math.min(image.width, image.height);
            const canvas = document.createElement('canvas');
            canvas.width = Math.min(side, 1024);
            canvas.height = canvas.width;
            const ctx = canvas.getContext('2d');
            if (!ctx) throw new Error(t('profile.preparePhotoFailed'));
            ctx.drawImage(
              image,
              (image.width - side) / 2,
              (image.height - side) / 2,
              side,
              side,
              0,
              0,
              canvas.width,
              canvas.height,
            );
            const blob = await new Promise<Blob>((done, fail) =>
              canvas.toBlob(
                (value) => (value ? done(value) : fail(new Error(t('profile.preparePhotoFailed')))),
                'image/jpeg',
                0.85,
              ),
            );
            const uri = URL.createObjectURL(blob);
            resolve({
              uri,
              dispose() {
                URL.revokeObjectURL(uri);
              },
            });
          } finally {
            image.close();
          }
        })().catch(reject);
      },
      { once: true },
    );
    input.click();
  });
}
