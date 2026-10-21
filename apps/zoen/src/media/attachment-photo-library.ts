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
  MAX_ATTACHMENT_ITEMS,
  MAX_ATTACHMENT_TOTAL_BYTES,
  type AttachmentAsset,
} from './attachment-draft';
import { discardCacheCopy } from './cache-files';

/** UI metadata uses seconds; SDK58's metadata mappers return milliseconds. */
export type PhotoLibraryItem = AssetMetadata;
export type PhotoLibraryAlbum = Pick<AlbumMetadata, 'id' | 'title'> & {
  count?: number;
  previewId?: string | null;
};
export type PhotoLibraryPermission = Pick<
  PermissionResponse,
  'granted' | 'canAskAgain' | 'status' | 'accessPrivileges'
>;
function normalizedMetadata(item: PhotoLibraryItem): PhotoLibraryItem {
  return {
    ...item,
    duration:
      item.duration !== null && Number.isFinite(item.duration) && item.duration >= 0
        ? item.duration / 1000
        : null,
  };
}

/** Refresh only the selected/current identities, without resolving their files.
 * SDK58's Query has no ID predicate. A fresh Asset lazily fetches one PHAsset,
 * then caches that immutable metadata for its cheap getters. Never use getInfo:
 * it also resolves CURRENT and may download/export a video. */
async function refreshLibraryItems(
  items: readonly PhotoLibraryItem[],
): Promise<PhotoLibraryItem[]> {
  if (
    items.length > MAX_ATTACHMENT_ITEMS + 1 ||
    new Set(items.map((item) => item.id)).size !== items.length ||
    items.some(
      (item) =>
        !item.id.startsWith('ph://') ||
        item.id.length <= 5 ||
        item.id.length > 2048 ||
        item.id.includes('\0'),
    )
  )
    throw new AttachmentDraftFailure('invalid');
  const refreshed = await Promise.all(
    items.map(async (item) => {
      const asset = new Asset(item.id);
      try {
        // Load the fresh PHAsset before the remaining getters share its cache.
        const mediaType = await asset.getMediaType();
        if (mediaType !== MediaType.IMAGE && mediaType !== MediaType.VIDEO) return null;
        const [
          filename,
          shape,
          duration,
          creationTime,
          modificationTime,
          isFavorite,
          mediaSubtypes,
        ] = await Promise.all([
          asset.getFilename(),
          asset.getShape(),
          asset.getDuration(),
          asset.getCreationTime(),
          asset.getModificationTime(),
          asset.getFavorite(),
          asset.getMediaSubtypes(),
        ]);
        return normalizedMetadata({
          id: asset.id,
          filename,
          mediaType,
          width: shape?.width ?? null,
          height: shape?.height ?? null,
          duration,
          creationTime,
          modificationTime,
          isFavorite,
          mediaSubtypes,
        });
      } catch (error) {
        // This is the pinned SDK's missing/revoked-PHAsset exception. A transient
        // read failure is not evidence that a selected photo was deleted.
        if (
          error &&
          typeof error === 'object' &&
          'code' in error &&
          error.code === 'ERR_ASSET_NOT_FOUND'
        )
          return null;
        throw error;
      }
    }),
  );
  return refreshed.filter((item): item is PhotoLibraryItem => item !== null);
}
export const photoLibrary = {
  permission: getPermissionsAsync,
  requestPermission: requestPermissionsAsync,
  manage: () => presentPermissionsPicker(['photo', 'video']),
  subscribe: (reload: () => void) => {
    const subscription = addListener(reload);
    return () => subscription.remove();
  },
  albums: () => Album.getAlbumsMetadata(),
  refresh: refreshLibraryItems,
  albumDetails: async (id: string) => {
    const album = new Album(id);
    const [count, items] = await Promise.all([
      album.getAssetCount(),
      new Query()
        .within(AssetField.MEDIA_TYPE, [MediaType.IMAGE, MediaType.VIDEO])
        .orderBy({ key: AssetField.CREATION_TIME, ascending: false })
        .album(album)
        .limit(1)
        .exeForMetadata(),
    ]);
    return { count, previewId: items[0]?.id ?? null };
  },
  load: async (albumId: string | null, offset: number) => {
    const query = new Query()
      .within(AssetField.MEDIA_TYPE, [MediaType.IMAGE, MediaType.VIDEO])
      .orderBy({ key: AssetField.CREATION_TIME, ascending: false })
      .limit(60)
      .offset(offset);
    if (albumId) query.album(new Album(albumId));
    const items = await query.exeForMetadata();
    return items.map(normalizedMetadata);
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
          await source.copy(destination);
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
