import {
  Album,
  Asset,
  AssetField,
  AssetUriVersion,
  MediaType,
  Query,
  getPermissionsAsync,
  requestPermissionsAsync,
  presentPermissionsPicker,
  addListener,
  type AlbumMetadata,
  type AssetMetadata,
  type PermissionResponse,
} from 'expo-media-library';
import { File, Paths } from 'expo-file-system';
import CoreModule from '../../modules/zoen-core/src/ZoenCoreModule';
import { randomUUID } from 'expo-crypto';
import {
  AttachmentDraftFailure,
  MAX_ATTACHMENT_TOTAL_BYTES,
  type AttachmentAsset,
} from './attachment-draft';
import { discardCacheCopy } from './cache-files';

export type PhotoLibraryItem = AssetMetadata;
export type PhotoLibraryAlbum = Pick<AlbumMetadata, 'id' | 'title'>;
export type PhotoLibraryPermission = Pick<
  PermissionResponse,
  'granted' | 'canAskAgain' | 'status' | 'accessPrivileges'
>;
export const photoLibrary = {
  permission: getPermissionsAsync,
  requestPermission: requestPermissionsAsync,
  manage: () => presentPermissionsPicker(['photo', 'video']),
  subscribe: (reload: () => void) => {
    const subscription = addListener(reload);
    return () => subscription.remove();
  },
  albums: () => Album.getAlbumsMetadata(),
  load: (albumId: string | null, offset: number) => {
    const query = new Query()
      .within(AssetField.MEDIA_TYPE, [MediaType.IMAGE, MediaType.VIDEO])
      .orderBy({ key: AssetField.CREATION_TIME, ascending: false })
      .limit(60)
      .offset(offset);
    if (albumId) query.album(new Album(albumId));
    return query.exeForMetadata();
  },
  materialize: materializeLibrarySelection,
  discard: discardCacheCopy,
};

/** Photos URLs remain library-owned. Only our fresh cache copies enter the existing draft owner. */
export async function materializeLibrarySelection(
  items: readonly PhotoLibraryItem[],
  current: () => boolean,
): Promise<AttachmentAsset[]> {
  if (!items.length || !current()) return [];
  const nativeRelease = CoreModule?.releaseMediaLibraryExport?.bind(CoreModule);
  if (!nativeRelease) throw new AttachmentDraftFailure('unavailable');
  const release = async (uri: string) => {
    try {
      await nativeRelease(uri);
    } catch (error) {
      throw new AttachmentDraftFailure('cleanup', error);
    }
  };
  const owned: AttachmentAsset[] = [];
  try {
    for (const item of items) {
      if (!current()) break;
      const asset = new Asset(item.id);
      // CURRENT includes Photos edits, including native trimmed/slow-motion video exports.
      let uri: string | null = null;
      try {
        uri = await asset.getUri({ version: AssetUriVersion.CURRENT });
        if (!current()) break;
        if (!uri.startsWith('file://')) throw new AttachmentDraftFailure('invalid');
        const source = new File(uri);
        if (
          !source.exists ||
          !Number.isSafeInteger(source.size) ||
          source.size <= 0 ||
          source.size + owned.reduce((sum, file) => sum + file.size, 0) > MAX_ATTACHMENT_TOTAL_BYTES
        )
          throw new AttachmentDraftFailure('size');
        const mime = source.type;
        if (
          !mime ||
          !(item.mediaType === MediaType.IMAGE
            ? mime.startsWith('image/')
            : item.mediaType === MediaType.VIDEO && mime.startsWith('video/'))
        )
          throw new AttachmentDraftFailure('invalid');
        // The resolved CURRENT file may have a different extension/format than the original asset.
        const extension =
          /\.[a-zA-Z0-9]{1,8}$/.exec(source.name)?.[0] ??
          (mime.startsWith('video/') ? '.mov' : '.jpg');
        const destination = new File(Paths.cache, `attachment-${randomUUID()}${extension}`);
        try {
          source.copy(destination);
          const copy = { uri: destination.uri, name: source.name, mime, size: destination.size };
          owned.push(copy);
          if (
            !destination.exists ||
            !Number.isSafeInteger(copy.size) ||
            copy.size !== source.size ||
            owned.reduce((sum, file) => sum + file.size, 0) > MAX_ATTACHMENT_TOTAL_BYTES
          )
            throw new AttachmentDraftFailure('size');
        } catch (error) {
          discardCacheCopy(destination.uri);
          throw error;
        }
      } finally {
        // Native validation recognizes only the pinned SDK's fresh export namespace/name.
        // Photos/AVURLAsset originals are preserved. This URL came from this exact getUri.
        if (uri) await release(uri);
      }
    }
    if (current()) return owned;
  } catch (error) {
    for (const item of owned) discardCacheCopy(item.uri);
    throw error;
  }
  for (const item of owned) discardCacheCopy(item.uri);
  return [];
}
