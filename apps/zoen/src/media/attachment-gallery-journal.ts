import { MediaType } from 'expo-media-library';
import type { AttachmentAsset } from './attachment-draft';
import type { PhotoLibraryItem } from './attachment-photo-library';
import type {
  PhotoPickerPort,
  PhotoPickerState,
  PhotoPickerSubmission,
} from './attachment-photo-types';
import { sensitiveCacheWork } from './cache-work';
import { contentValue } from './media-draft-contract';
import { MediaDraftFailure, type MediaDraftChannel } from './media-draft-channel';
import { copyMediaReply, type MediaReplyFields } from './media-reply';
import type {
  MaterializedMediaDraftAsset,
  MediaDraftAttachments,
  MediaDraftIngest,
} from './media-draft-types';

type Captions = PhotoPickerState['captions'];
type Desired = { selected: readonly PhotoLibraryItem[]; captions: Captions };
type Copy = { id: string; file: AttachmentAsset | null };
/** Replaceable Photos projection. Every accepted byte, caption and order lives in the existing journal. */
export class AttachmentGalleryJournal {
  private desired: Desired | null = null;
  private running: Promise<void> | null = null;
  private stopped = false;
  private copies = new Map<string, Copy>();
  private leases = new Map<string, string>();
  private previews = new Map<string, Promise<MaterializedMediaDraftAsset>>();
  private port: PhotoPickerPort | null = null;
  private readonly baseline: MediaDraftAttachments | null;
  private readonly replyContext: MediaReplyFields;
  private readonly replyTo: string | null;
  constructor(
    private readonly channel: MediaDraftChannel,
    private readonly current: () => boolean,
    private readonly id: () => string,
    replyTo: string | null,
    replyContext: MediaReplyFields = {},
  ) {
    const value = this.attachments();
    this.replyTo = value ? value.reply_to : replyTo;
    this.replyContext = copyMediaReply(value ?? replyContext);
    const ids = new Set(value?.gallery?.entries.map((entry) => entry.asset_id));
    const items = value?.items.filter((item) => !ids.has(item.asset_id)) ?? [];
    this.baseline =
      value && items.length
        ? {
            kind: 'attachments',
            reply_to: value.reply_to,
            ...copyMediaReply(value),
            items,
            ...(value.options ? { options: value.options } : {}),
          }
        : null;
    for (const entry of value?.gallery?.entries ?? [])
      this.copies.set(entry.photo_id, { id: entry.asset_id, file: null });
  }
  private attachments() {
    const state = this.channel.getSnapshot();
    if (
      !this.current() ||
      state.phase !== 'ready' ||
      state.frozen ||
      (state.content && state.content.kind !== 'attachments')
    )
      throw new MediaDraftFailure('closed');
    return state.content?.kind === 'attachments' ? state.content : null;
  }
  private checkBaseline(value: MediaDraftAttachments | null) {
    const pending = new Set(value?.gallery?.entries.map((entry) => entry.asset_id));
    const items = value?.items.filter((item) => !pending.has(item.asset_id)) ?? [];
    const actual =
      value && items.length
        ? {
            kind: value.kind,
            reply_to: value.reply_to,
            ...copyMediaReply(value),
            items,
            ...(value.options ? { options: value.options } : {}),
          }
        : null;
    if (JSON.stringify(actual) !== JSON.stringify(this.baseline))
      throw new MediaDraftFailure('conflict');
  }
  restore() {
    const value = this.attachments();
    const assets = this.channel.getSnapshot().value?.assets ?? [];
    const selected = (value?.gallery?.entries ?? []).map((entry): PhotoLibraryItem => {
      const asset = assets.find((asset) => asset.id === entry.asset_id);
      if (!asset || (!asset.mime.startsWith('image/') && !asset.mime.startsWith('video/')))
        throw new MediaDraftFailure('invalid');
      return {
        id: entry.photo_id,
        filename: asset.name,
        mediaType: asset.mime.startsWith('video/') ? MediaType.VIDEO : MediaType.IMAGE,
        width: null,
        height: null,
        duration: null,
        creationTime: null,
        modificationTime: null,
        isFavorite: false,
        mediaSubtypes: [],
      };
    });
    const captions = Object.fromEntries(
      (value?.gallery?.entries ?? []).map((entry) => [
        entry.photo_id,
        value!.items.find((item) => item.asset_id === entry.asset_id)!.caption,
      ]),
    );
    return {
      selected,
      captions: {
        forced: value?.gallery?.forced_caption
          ? (value.items.find((item) => item.asset_id === value.gallery?.entries[0]?.asset_id)
              ?.caption ?? '')
          : null,
        items: captions,
      },
    };
  }
  update(selected: readonly PhotoLibraryItem[], captions: Captions, port: PhotoPickerPort) {
    if (this.stopped || !this.current()) return Promise.resolve();
    this.port = port;
    this.desired = {
      selected: [...selected],
      captions: { forced: captions.forced, items: { ...captions.items } },
    };
    if (!this.running) {
      const task = this.channel.stage((save) =>
        sensitiveCacheWork.run(async () => {
          while (this.desired && !this.stopped) {
            const desired = this.desired;
            this.checkBaseline(this.attachments());
            const missing = desired.selected.filter((item) => !this.copies.has(item.id));
            if (missing.length) {
              const files = await port.materialize(missing, () => this.current() && !this.stopped);
              if (!this.current() || this.stopped) {
                files.forEach((file) => port.discard(file.uri));
                return;
              }
              if (files.length !== missing.length) {
                files.forEach((file) => port.discard(file.uri));
                throw new MediaDraftFailure('invalid');
              }
              missing.forEach((item, index) =>
                this.copies.set(item.id, { id: this.id(), file: files[index]! }),
              );
            }
            if (desired !== this.desired) continue;
            const selectedIds = new Set(desired.selected.map((item) => item.id));
            for (const [photo, copy] of this.copies)
              if (!selectedIds.has(photo)) {
                if (copy.file) port.discard(copy.file.uri);
                this.copies.delete(photo);
              }
            this.checkBaseline(this.attachments());
            const entries = desired.selected.map((item) => ({
              asset_id: this.copies.get(item.id)!.id,
              photo_id: item.id,
            }));
            const items = [
              ...(this.baseline?.items ?? []),
              ...entries.map((entry, index) => ({
                asset_id: entry.asset_id,
                mutation_id: entry.asset_id,
                caption:
                  desired.captions.forced !== null
                    ? index === 0
                      ? desired.captions.forced
                      : ''
                    : (desired.captions.items[entry.photo_id] ?? ''),
              })),
            ];
            const next: MediaDraftAttachments | null = items.length
              ? {
                  kind: 'attachments',
                  reply_to: this.baseline ? this.baseline.reply_to : this.replyTo,
                  ...this.replyContext,
                  items,
                  ...(this.baseline?.options ? { options: this.baseline.options } : {}),
                  ...(entries.length
                    ? { gallery: { entries, forced_caption: desired.captions.forced !== null } }
                    : {}),
                }
              : null;
            const inputs: MediaDraftIngest[] = entries.flatMap((entry) => {
              const file = this.copies.get(entry.photo_id)!.file;
              return file
                ? [{ id: entry.asset_id, uri: file.uri, name: file.name, mime: file.mime }]
                : [];
            });
            if (
              inputs.length ||
              JSON.stringify(this.channel.getSnapshot().content) !== contentValue(next).json
            )
              await save(next, inputs);
            for (const entry of entries) {
              const copy = this.copies.get(entry.photo_id)!;
              if (copy.file) {
                port.discard(copy.file.uri);
                copy.file = null;
              }
            }
            if (this.desired === desired) this.desired = null;
          }
        }),
      );
      this.running = task;
      void task.then(
        () => {
          if (this.running === task) this.running = null;
        },
        () => {
          if (this.running === task) this.running = null;
        },
      );
    }
    return this.running;
  }
  async retry() {
    if (this.stopped || !this.current()) throw new MediaDraftFailure('closed');
    await this.channel.retry();
    const assets = this.channel.getSnapshot().value?.assets ?? [];
    for (const copy of this.copies.values())
      if (copy.file && assets.some((asset) => asset.id === copy.id)) {
        this.port?.discard(copy.file.uri);
        copy.file = null;
      }
    const desired = this.desired;
    if (desired && this.port) await this.update(desired.selected, desired.captions, this.port);
    await this.flush();
  }
  async flush() {
    if (this.running) await this.running;
    await this.channel.flush();
  }
  async materialize(selected: readonly PhotoLibraryItem[]) {
    await this.flush();
    const value = this.attachments();
    if (
      !value?.gallery ||
      selected.length !== value.gallery.entries.length ||
      selected.some((item, index) => item.id !== value.gallery!.entries[index]?.photo_id)
    )
      throw new MediaDraftFailure('conflict');
    const files: AttachmentAsset[] = [];
    for (const entry of value.gallery.entries) {
      let pending = this.previews.get(entry.asset_id);
      if (!pending) {
        pending = this.channel.materialize(entry.asset_id);
        this.previews.set(entry.asset_id, pending);
        void pending.catch(() => {
          if (this.previews.get(entry.asset_id) === pending) this.previews.delete(entry.asset_id);
        });
      }
      const file = await pending;
      if (!this.current()) throw new MediaDraftFailure('closed');
      this.leases.set(file.uri, file.id);
      files.push(file);
    }
    return files;
  }
  retained() {
    return this.restore().selected;
  }
  async preview(selected: readonly PhotoLibraryItem[]) {
    const files = await this.materialize(selected);
    return Object.fromEntries(selected.map((item, index) => [item.id, files[index]!.uri]));
  }
  owns() {
    const value = this.attachments();
    this.checkBaseline(value);
    return Boolean(
      value?.gallery &&
      value.gallery.entries.every(
        (entry) => this.copies.get(entry.photo_id)?.id === entry.asset_id,
      ),
    );
  }
  basis() {
    return this.baseline;
  }
  async finish(
    files: readonly AttachmentAsset[],
    submission: PhotoPickerSubmission | null,
    preferred: string | null,
  ) {
    await this.flush();
    const latest = this.attachments();
    if (!latest?.gallery) return null;
    this.checkBaseline(latest);
    const gallery = latest.gallery;
    const known = files.filter((file) => this.leases.has(file.uri));
    if (
      known.length !== gallery.entries.length ||
      known.some((file, index) => this.leases.get(file.uri) !== gallery.entries[index]?.asset_id)
    )
      throw new MediaDraftFailure('conflict');
    const fresh = files
      .filter((file) => !this.leases.has(file.uri))
      .map((file) => ({ ...file, id: this.id() }));
    const selected = [
      ...gallery.entries.map((entry, index) => ({
        ...latest.items.find((item) => item.asset_id === entry.asset_id)!,
        caption:
          submission?.captions?.[index] ??
          (index === 0 ? submission?.caption : undefined) ??
          latest.items.find((item) => item.asset_id === entry.asset_id)!.caption,
      })),
      ...fresh.map((file) => ({ asset_id: file.id, mutation_id: file.id, caption: '' })),
    ];
    const options = this.baseline?.options ?? submission?.options;
    const next: MediaDraftAttachments = {
      kind: 'attachments',
      reply_to: latest.reply_to,
      ...copyMediaReply(latest),
      items: [...(this.baseline?.items ?? []), ...selected],
      ...(options ? { options } : {}),
    };
    await this.channel.save(
      next,
      fresh.map(({ id, uri, name, mime }) => ({ id, uri, name, mime })),
    );
    fresh.forEach((file) => this.port?.discard(file.uri));
    this.stopped = true;
    return {
      content: next,
      selectedId:
        (preferred ? this.leases.get(preferred) : null) ?? gallery.entries[0]?.asset_id ?? null,
    };
  }
  async cancel() {
    this.stopped = true;
    this.desired = null;
    if (this.running) await this.running.catch(() => {});
    for (const copy of this.copies.values()) if (copy.file) this.port?.discard(copy.file.uri);
    if (!this.current()) return;
    const value = this.attachments();
    if (!value?.gallery) return;
    this.checkBaseline(value);
    await this.channel.save(this.baseline);
  }
}
