import type { ImageManipulatorContext, ImageRef } from 'expo-image-manipulator';
import { discardCacheCopy } from '../media/cache-files';
import { sensitiveCacheWork } from '../media/cache-work';
import type { PhotoDraft } from './types';

/** A local, metadata-free draft. Rust is changed only by the editor's Save action. */
export function pickProfilePhoto(): Promise<PhotoDraft | null> {
  return sensitiveCacheWork.run(async () => {
    const { launchImageLibraryAsync } = await import('expo-image-picker');
    const picked = await launchImageLibraryAsync({
      mediaTypes: ['images'],
      allowsMultipleSelection: false,
      exif: false,
      base64: false,
    });
    if (picked.canceled || !picked.assets[0]) return null;
    const source = picked.assets[0].uri;
    let context: ImageManipulatorContext | undefined;
    let image: ImageRef | undefined;
    let cropped: ImageManipulatorContext | undefined;
    let output: ImageRef | undefined;
    let savedUri: string | undefined;
    let retained = false;
    try {
      const { ImageManipulator, SaveFormat } = await import('expo-image-manipulator');
      context = ImageManipulator.manipulate(source, { maxWidth: 1024, maxHeight: 1024 });
      image = await context.renderAsync();
      const side = Math.min(image.width, image.height);
      cropped = ImageManipulator.manipulate(image).crop({
        originX: Math.floor((image.width - side) / 2),
        originY: Math.floor((image.height - side) / 2),
        width: side,
        height: side,
      });
      output = await cropped.renderAsync();
      const saved = await output.saveAsync({ format: SaveFormat.JPEG, compress: 0.85 });
      savedUri = saved.uri;
      // ImagePicker creates a separate app-cache copy. It is no longer needed.
      discardCacheCopy(source);
      retained = true;
      return {
        uri: saved.uri,
        dispose() {
          try {
            discardCacheCopy(saved.uri);
          } catch {
            /* OS cache eviction is harmless; device erasure also sweeps this namespace. */
          }
        },
      };
    } finally {
      output?.release();
      cropped?.release();
      image?.release();
      context?.release();
      if (!retained) {
        try {
          discardCacheCopy(source);
        } finally {
          discardCacheCopy(savedUri);
        }
      }
    }
  });
}
