import type { AttachmentGalleryJournal } from './attachment-gallery-journal.ts';
import type { AttachmentAsset } from './attachment-draft';
import type {
  PhotoLibraryAlbum,
  PhotoLibraryItem,
  PhotoLibraryPermission,
} from './attachment-photo-library';
import type { MediaDraftAttachmentOptions } from './media-draft-types';
export type PhotoPickerState = Readonly<{
  token: number;
  visible: boolean;
  busy: boolean;
  permission: PhotoLibraryPermission | null;
  items: readonly PhotoLibraryItem[];
  selected: readonly PhotoLibraryItem[];
  albums: readonly PhotoLibraryAlbum[];
  albumId: string | null;
  loading: boolean;
  hasMore: boolean;
  error: unknown;
  screen: 'grid' | 'albums' | 'preview' | 'selected';
  previewId: string | null;
  previewUris: Readonly<Record<string, string | null>>;
  captions: Readonly<{ forced: string | null; items: Readonly<Record<string, string>> }>;
  category: 'photos' | 'document' | 'audio';
}>;
export type PhotoPickerPort = {
  permission(): Promise<PhotoLibraryPermission>;
  requestPermission(): Promise<PhotoLibraryPermission>;
  load(albumId: string | null, offset: number): Promise<PhotoLibraryItem[]>;
  refresh(items: readonly PhotoLibraryItem[]): Promise<PhotoLibraryItem[]>;
  albums(): Promise<PhotoLibraryAlbum[]>;
  albumDetails?(id: string): Promise<{ count: number; previewId: string | null }>;
  manage(): Promise<void>;
  materialize(
    items: readonly PhotoLibraryItem[],
    current: () => boolean,
  ): Promise<AttachmentAsset[]>;
  capture(remaining: number, current: () => boolean): Promise<AttachmentAsset[]>;
  browse?(
    remaining: number,
    current: () => boolean,
    category?: 'document' | 'audio',
  ): Promise<AttachmentAsset[]>;
  discard(uri: string): void;
};
export type PhotoPickerSubmission = Readonly<{
  caption: string;
  /** Ordered exactly like the materialized selection; never addressed by filename. */
  captions?: readonly string[];
  send: boolean;
  requestEdit?: boolean;
  options?: MediaDraftAttachmentOptions;
}>;
/** Derived by the current durable draft owner; never a second draft authority. */
export type PhotoPickerAdmission = Readonly<{
  journal?: AttachmentGalleryJournal;
  sendAsFileAllowed: boolean;
  existingOptions?: MediaDraftAttachmentOptions;
}>;
export type Request = {
  journal?: AttachmentGalleryJournal;
  current(): boolean;
  remaining: number;
  resolve(files: AttachmentAsset[]): void;
  reject(error: unknown): void;
  retired: boolean;
  discarding?: boolean;
  working: boolean;
  epoch: number;
  needsReconciliation: boolean;
  pageOffset: number;
  action: 'camera' | 'document' | 'recentPreview' | 'route' | null;
  route?: (current: () => boolean) => void;
  preview?: (current: () => boolean) => Promise<void>;
  result: AttachmentAsset[] | null;
  albumDetails: Map<string, object>;
};
export const empty: PhotoPickerState = {
  token: 0,
  visible: false,
  busy: false,
  permission: null,
  items: [],
  selected: [],
  albums: [],
  albumId: null,
  loading: false,
  hasMore: false,
  error: null,
  screen: 'grid',
  previewId: null,
  previewUris: {},
  captions: { forced: null, items: {} },
  category: 'photos',
};
