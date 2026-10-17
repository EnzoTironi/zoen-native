import type { PhotoDraft } from './types';

/** A local, metadata-free draft. Rust is changed only by the editor's Save action. */
export async function pickProfilePhoto(): Promise<PhotoDraft | null> {
  const { launchImageLibraryAsync } = await import('expo-image-picker');
  const picked = await launchImageLibraryAsync({
    mediaTypes: ['images'],
    allowsMultipleSelection: false,
    exif: false,
    base64: false,
  });
  if (picked.canceled || !picked.assets[0]) return null;
  const { ImageManipulator, SaveFormat } = await import('expo-image-manipulator');
  const { File } = await import('expo-file-system');
  const context = ImageManipulator.manipulate(picked.assets[0].uri, {
    maxWidth: 1024,
    maxHeight: 1024,
  });
  const image = await context.renderAsync();
  const side = Math.min(image.width, image.height);
  const cropped = ImageManipulator.manipulate(image).crop({
    originX: Math.floor((image.width - side) / 2),
    originY: Math.floor((image.height - side) / 2),
    width: side,
    height: side,
  });
  const output = await cropped.renderAsync();
  try {
    const saved = await output.saveAsync({ format: SaveFormat.JPEG, compress: 0.85 });
    const file = new File(saved.uri);
    return {
      uri: saved.uri,
      dispose() {
        try {
          if (file.exists) file.delete();
        } catch {
          /* cache eviction is harmless */
        }
      },
    };
  } finally {
    output.release();
    cropped.release();
    image.release();
    context.release();
  }
}
