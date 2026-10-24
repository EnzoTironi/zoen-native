import { galleryValue } from './media-draft-gallery.ts';
import type { MediaDraftChannel } from './media-draft-channel.ts';
import type {
  MediaDraftAsset,
  MediaDraftAttachmentOptions,
  MediaDraftContent,
  MediaDraftDto,
  MediaDraftIngest,
} from './media-draft-types.ts';

export type MediaDraftState = Readonly<{
  phase: 'loading' | 'ready' | 'closed';
  value: MediaDraftDto | null;
  content: MediaDraftContent | null;
  saving: boolean;
  sending: boolean;
  reviewing: boolean;
  frozen: boolean;
  error: Error | null;
}>;
export class MediaDraftFailure extends Error {
  readonly kind: 'closed' | 'notLoaded' | 'invalid' | 'busy' | 'attempted' | 'conflict';
  constructor(kind: MediaDraftFailure['kind'], cause?: unknown) {
    super(cause instanceof Error ? cause.message : `Media draft ${kind}`, { cause });
    this.name = 'MediaDraftFailure';
    this.kind = kind;
  }
}
export function invalid(): never {
  throw new MediaDraftFailure('invalid');
}
export const bytes = (text: string) => {
  let count = 0;
  for (const letter of text) {
    const point = letter.codePointAt(0)!;
    count += point < 128 ? 1 : point < 2048 ? 2 : point < 65536 ? 3 : 4;
  }
  return count;
};
export const id = (value: unknown): value is string =>
  typeof value === 'string' && /^[A-Za-z0-9_-]{1,128}$/.test(value);
export const boundedText = (value: unknown, limit: number): value is string =>
  typeof value === 'string' && !value.includes('\0') && bytes(value) <= limit;
export const hasControl = (value: string) =>
  [...value].some((char) => char.charCodeAt(0) < 32 || char.charCodeAt(0) === 127);
export const target = (value: unknown): value is string | null =>
  value === null || (boundedText(value, 256) && value.length > 0);
export const object = (value: unknown): Record<string, unknown> => {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return invalid();
  return value as Record<string, unknown>;
};
export function keys(value: Record<string, unknown>, expected: string[]) {
  if (Object.keys(value).sort().join(',') !== expected.sort().join(',')) invalid();
}
export function freeze<T>(value: T): T {
  if (value && typeof value === 'object') {
    for (const child of Object.values(value)) freeze(child);
    Object.freeze(value);
  }
  return value;
}
/** Opaque editor data remains JSON, never an SDK object, cache path or file lease. */
export function editorJson(value: unknown, depth = 0, seen = new Set<unknown>()): unknown {
  if (depth > 64) invalid();
  if (value === null || typeof value === 'boolean') return value;
  if (typeof value === 'number') return Number.isFinite(value) ? value : invalid();
  if (typeof value === 'string') {
    if (/^(?:file:|content:|blob:|ph:|assets-library:|\/private\/|\/var\/|\/Users\/)/i.test(value))
      invalid();
    return value;
  }
  if (!value || typeof value !== 'object' || seen.has(value)) return invalid();
  const proto = Object.getPrototypeOf(value);
  if (!Array.isArray(value) && proto !== Object.prototype && proto !== null) invalid();
  seen.add(value);
  let result: unknown;
  if (Array.isArray(value)) result = value.map((child) => editorJson(child, depth + 1, seen));
  else {
    const record: Record<string, unknown> = Object.create(null) as Record<string, unknown>;
    for (const [key, child] of Object.entries(value).sort(([left], [right]) =>
      left.localeCompare(right),
    )) {
      if (/(?:^|_)(?:uri|url|path|lease)(?:$|_)|(?:Uri|URL|Path|Lease)$/.test(key)) invalid();
      record[key] = editorJson(child, depth + 1, seen);
    }
    result = record;
  }
  seen.delete(value);
  return result;
}
export function contentValue(input: unknown): {
  content: MediaDraftContent | null;
  json: string;
  refs: string[];
} {
  if (input === null) return { content: null, json: 'null', refs: [] };
  const raw = object(input);
  const refs: string[] = [];
  if (!target(raw.reply_to)) invalid();
  let content: MediaDraftContent;
  if (raw.kind === 'attachments') {
    const hasOptions = Object.hasOwn(raw, 'options');
    const hasGallery = Object.hasOwn(raw, 'gallery');
    keys(raw, [
      'kind',
      'reply_to',
      'items',
      ...(hasOptions ? ['options'] : []),
      ...(hasGallery ? ['gallery'] : []),
    ]);
    if (!Array.isArray(raw.items) || !raw.items.length || raw.items.length > 10) invalid();
    const items = raw.items.map((item: unknown) => {
      const entry = object(item);
      keys(entry, ['asset_id', 'caption', 'mutation_id']);
      if (!id(entry.asset_id) || !id(entry.mutation_id) || !boundedText(entry.caption, 16 * 1024))
        return invalid();
      refs.push(entry.asset_id);
      return { asset_id: entry.asset_id, caption: entry.caption, mutation_id: entry.mutation_id };
    });
    if (new Set(items.map((item) => item.mutation_id)).size !== items.length) invalid();
    let options: MediaDraftAttachmentOptions | undefined;
    if (hasOptions) {
      const value = object(raw.options);
      keys(value, ['presentation', 'grouping']);
      if (
        (value.presentation !== 'media' && value.presentation !== 'file') ||
        (value.grouping !== 'separate' && value.grouping !== 'grouped') ||
        (value.grouping === 'grouped' && items.length < 2)
      )
        invalid();
      // Keep legacy content JSON unchanged so its durable save receipts remain valid.
      if (value.presentation !== 'media' || value.grouping !== 'separate')
        options = { presentation: value.presentation, grouping: value.grouping };
    }
    content = {
      kind: 'attachments',
      reply_to: raw.reply_to,
      items,
      ...(options ? { options } : {}),
      ...(hasGallery ? { gallery: galleryValue(raw.gallery, items) } : {}),
    };
  } else if (raw.kind === 'voice') {
    keys(raw, ['kind', 'reply_to', 'source_asset_id', 'editor', 'render']);
    if (!id(raw.source_asset_id)) invalid();
    refs.push(raw.source_asset_id);
    let render = null;
    if (raw.render !== null) {
      const entry = object(raw.render);
      keys(entry, ['asset_id', 'mime', 'duration_ms', 'levels', 'transcript']);
      if (
        !id(entry.asset_id) ||
        !boundedText(entry.mime, 255) ||
        !entry.mime.startsWith('audio/') ||
        typeof entry.duration_ms !== 'number' ||
        !Number.isFinite(entry.duration_ms) ||
        entry.duration_ms < 250 ||
        entry.duration_ms > 3_600_000 ||
        !Array.isArray(entry.levels) ||
        entry.levels.length > 160 ||
        !entry.levels.every(
          (level) =>
            typeof level === 'number' && Number.isFinite(level) && level >= 0 && level <= 1,
        ) ||
        !boundedText(entry.transcript, 65536)
      )
        return invalid();
      refs.push(entry.asset_id);
      render = {
        asset_id: entry.asset_id,
        mime: entry.mime,
        duration_ms: entry.duration_ms,
        levels: [...entry.levels] as number[],
        transcript: entry.transcript,
      };
    }
    content = {
      kind: 'voice',
      reply_to: raw.reply_to,
      source_asset_id: raw.source_asset_id as string,
      editor: editorJson(raw.editor),
      render,
    };
  } else return invalid();
  if (new Set(refs).size !== refs.length) invalid();
  const json = JSON.stringify(content);
  if (bytes(json) > 512 * 1024) invalid();
  return { content: freeze(content), json, refs };
}
export function assetValue(input: unknown): MediaDraftAsset {
  const value = object(input);
  keys(value, ['id', 'name', 'mime', 'size', 'sha256']);
  if (
    !id(value.id) ||
    !boundedText(value.name, 1024) ||
    !value.name.length ||
    /[/\\]/.test(value.name) ||
    !boundedText(value.mime, 255) ||
    !value.mime.includes('/') ||
    hasControl(value.mime) ||
    typeof value.size !== 'number' ||
    !Number.isSafeInteger(value.size) ||
    value.size <= 0 ||
    value.size > 64 * 1024 * 1024 ||
    typeof value.sha256 !== 'string' ||
    !/^[a-f0-9]{64}$/.test(value.sha256)
  )
    return invalid();
  return Object.freeze({
    id: value.id,
    name: value.name,
    mime: value.mime,
    size: value.size,
    sha256: value.sha256,
  });
}
export function dtoValue(input: unknown): {
  value: MediaDraftDto;
  content: MediaDraftContent | null;
} {
  const raw = object(input);
  keys(raw, ['revision', 'content_json', 'assets', 'attempted']);
  if (
    !boundedText(raw.revision, 256) ||
    !raw.revision.length ||
    !boundedText(raw.content_json, 512 * 1024) ||
    !Array.isArray(raw.assets) ||
    typeof raw.attempted !== 'boolean'
  )
    return invalid();
  const parsed = contentValue(JSON.parse(raw.content_json));
  const assets = raw.assets.map(assetValue);
  if (
    assets.length !== parsed.refs.length ||
    assets.some((asset, index) => asset.id !== parsed.refs[index])
  )
    invalid();
  if (parsed.content?.kind === 'attachments' && parsed.content.gallery) {
    for (const entry of parsed.content.gallery.entries) {
      const asset = assets.find((asset) => asset.id === entry.asset_id);
      if (!asset || (!asset.mime.startsWith('image/') && !asset.mime.startsWith('video/')))
        invalid();
    }
  }
  const total = assets.reduce((count, asset) => count + asset.size, 0);
  if (total > (parsed.content?.kind === 'voice' ? 128 : 64) * 1024 * 1024) invalid();
  if (parsed.content?.kind === 'voice') {
    if (assets.some((asset) => !asset.mime.startsWith('audio/'))) invalid();
    if (parsed.content.render && parsed.content.render.mime !== assets[1]?.mime) invalid();
  }
  if (
    (parsed.content === null && raw.attempted) ||
    (raw.revision === '0' && parsed.content !== null)
  )
    invalid();
  return {
    value: freeze({
      revision: raw.revision,
      content_json: parsed.json,
      assets,
      attempted: raw.attempted,
    }),
    content: parsed.content,
  };
}
export function saveResultValue(input: unknown): {
  acceptedRevision: string;
  value: MediaDraftDto;
} {
  const raw = object(input);
  keys(raw, ['accepted_revision', 'draft']);
  if (!boundedText(raw.accepted_revision, 256) || !raw.accepted_revision.length) invalid();
  return { acceptedRevision: raw.accepted_revision, value: dtoValue(raw.draft).value };
}
export type Write = {
  content: MediaDraftContent | null;
  json: string;
  refs: string[];
  assets: readonly MediaDraftIngest[];
  baseline: MediaDraftDto | null;
  result: MediaDraftDto | null;
  acceptedRevision: string | null;
};
export type Intent = { revision: string };
export type Stage = {
  work: (save: MediaDraftChannel['save']) => Promise<void>;
  task: Promise<void> | null;
  active: boolean;
  cancelled: boolean;
  wrote: boolean;
  error: Error | null;
};
