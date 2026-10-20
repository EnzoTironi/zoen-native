import { Platform } from 'react-native';
import { File } from 'expo-file-system';
import * as DocumentPicker from 'expo-document-picker';
import * as ImagePicker from 'expo-image-picker';
import { t } from '../i18n';
import type { AttachmentAsset } from './attachment-draft';

export type AttachmentPickKind = 'document' | 'photo' | 'camera';

/** Every returned SDK cache copy becomes the caller's cleanup responsibility. */
export async function pickAttachmentAssets(
  kind: AttachmentPickKind,
  remaining: number,
  current: () => boolean,
): Promise<AttachmentAsset[]> {
  let assets: { uri: string; name: string; mime: string; size?: number }[] = [];
  if (kind === 'document') {
    const result = await DocumentPicker.getDocumentAsync({
      copyToCacheDirectory: true,
      multiple: Platform.OS === 'ios',
      type: '*/*',
      base64: false,
    });
    if (!result.canceled)
      assets = result.assets.map((file) => ({
        uri: file.uri,
        name: file.name,
        mime: file.mimeType || 'application/octet-stream',
        size: file.size,
      }));
  } else {
    if (kind === 'camera') {
      const permission = await ImagePicker.requestCameraPermissionsAsync();
      if (!current()) return [];
      if (!permission.granted) throw new Error(t('media.youCanAllowAccessInDeviceSettingsTo'));
    }
    const result =
      kind === 'camera'
        ? await ImagePicker.launchCameraAsync({
            mediaTypes: Platform.OS === 'ios' ? ['images', 'videos'] : ['images'],
            quality: 1,
            exif: false,
          })
        : await ImagePicker.launchImageLibraryAsync({
            mediaTypes: ['images', 'videos'],
            quality: 1,
            allowsMultipleSelection: Platform.OS === 'ios',
            selectionLimit: remaining,
            orderedSelection: true,
            exif: false,
          });
    if (!result.canceled)
      assets = result.assets.map((asset) => ({
        uri: asset.uri,
        name: asset.fileName ?? (asset.type === 'video' ? 'video.mov' : 'photo.jpg'),
        mime: asset.mimeType ?? (asset.type === 'video' ? 'video/quicktime' : 'image/jpeg'),
        size: asset.fileSize,
      }));
  }
  return assets.map((asset) => {
    let size = NaN;
    try {
      if (Platform.OS === 'web') size = asset.size ?? NaN;
      else {
        const file = new File(asset.uri);
        if (file.exists) size = file.size;
      }
    } catch {
      // Invalid metadata is rejected after the caller takes ownership of every URI.
    }
    return { ...asset, size };
  });
}
