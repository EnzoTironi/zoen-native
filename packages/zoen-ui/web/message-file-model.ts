// Pure boundary parsing; no session, transport or persistence owner.
import { object, string, integer, boolean, array } from './boundary.ts';

export function exactKeys(value: Record<string, unknown>, expected: readonly string[]) {
  if (Object.keys(value).sort().join(',') !== [...expected].sort().join(','))
    throw new Error('Invalid attachment metadata.');
}
export type MessageAttachmentMedia = Readonly<{
  presentation: 'media' | 'file';
  group: { id: string; index: number; count: number } | null;
}>;
export function parseMessageMedia(input: unknown): MessageAttachmentMedia | null {
  if (input === undefined || input === null) return null;
  const value = object(input);
  exactKeys(value, ['presentation', ...(Object.hasOwn(value, 'group') ? ['group'] : [])]);
  if (value.presentation !== 'media' && value.presentation !== 'file')
    throw new Error('Invalid attachment presentation.');
  let group: { id: string; index: number; count: number } | null = null;
  if (value.group !== undefined && value.group !== null) {
    const raw = object(value.group);
    exactKeys(raw, ['id', 'index', 'count']);
    const id = string(raw.id),
      index = integer(raw.index),
      count = integer(raw.count);
    if (!/^album_[a-f0-9]{64}$/.test(id) || count < 2 || count > 10 || index >= count)
      throw new Error('Invalid attachment group.');
    group = { id, index, count };
  }
  return { presentation: value.presentation, group };
}

export const HASH = /^[0-9a-f]{64}$/;
export function parseChunk(input: unknown) {
  const v = object(input);
  const chunk = {
    sha256: string(v.sha256),
    bytes: integer(v.bytes),
    blob: string(v.blob),
    key: string(v.key),
  };
  if (![chunk.sha256, chunk.blob, chunk.key].every((h) => HASH.test(h)))
    throw new Error('Invalid file chunk.');
  return chunk;
}
export function parseVoice(input: unknown) {
  const v = object(input);
  const reply = v.reply ? object(v.reply) : null;
  return {
    ms: integer(v.ms),
    levels: string(v.levels ?? ''),
    transcript: string(v.transcript ?? ''),
    reply: reply ? { to: string(reply.to), thread: boolean(reply.thread ?? false) } : null,
  };
}
export function parseFile(input: unknown) {
  const v = object(input);
  return {
    name: string(v.name),
    path: string(v.path ?? ''),
    mime: string(v.mime),
    bytes: integer(v.bytes),
    sha256: string(v.sha256),
    chunks: array(v.chunks).map(parseChunk),
    thumb: v.thumb ? parseChunk(v.thumb) : null,
    voice: v.voice ? parseVoice(v.voice) : null,
  };
}
export type FileDoc = ReturnType<typeof parseFile>;
export type Chunk = ReturnType<typeof parseChunk>;
export type VoiceMeta = ReturnType<typeof parseVoice>;
export type MessageMedia = NonNullable<ReturnType<typeof parseMessageMedia>>;

// Keep received media within the same bounds as client-core/src/files.rs.
export const MAX_FILE_BYTES = 512 << 20;
export const MAX_CHUNK_BYTES = 16 << 20;
export const MAX_THUMBNAIL_BYTES = 2 << 20;
export function boundedFile(input: FileDoc, maxChunk = MAX_CHUNK_BYTES): FileDoc {
  const invalid = () => new Error('Arquivo inválido ou grande demais para abrir.');
  if (!input.chunks.length || input.chunks.length > MAX_FILE_BYTES / (1 << 20) + 1) throw invalid();
  const file = parseFile(input); // Snapshot the manifest before any asynchronous work.
  if (!file.bytes || file.bytes > MAX_FILE_BYTES || !HASH.test(file.sha256)) throw invalid();
  if (
    file.chunks.some((c) => !c.bytes || c.bytes > maxChunk) ||
    file.chunks.reduce((total, c) => total + c.bytes, 0) !== file.bytes ||
    (file.thumb && (!file.thumb.bytes || file.thumb.bytes > MAX_THUMBNAIL_BYTES))
  )
    throw invalid();
  const metadata =
    new TextEncoder().encode(file.name + file.mime + file.sha256).length +
    (file.chunks.length + Number(!!file.thumb)) * (3 * 64 + 8);
  if (metadata > 128 << 10) throw invalid();
  return file;
}

/** Exact user selection intent; keep this object unchanged for a failed Send retry. */
export type AttachmentSendOptions = Readonly<{
  presentation: 'media' | 'file';
  grouping: 'separate' | 'grouped';
}>;
export function attachmentOptions(
  input: unknown,
  count: number,
): AttachmentSendOptions | undefined {
  if (count < 1 || count > 10) throw new Error('Selecione de um a dez arquivos.');
  if (input === undefined) return undefined;
  const value = object(input);
  exactKeys(value, ['presentation', 'grouping']);
  if (
    (value.presentation !== 'media' && value.presentation !== 'file') ||
    (value.grouping !== 'separate' && value.grouping !== 'grouped') ||
    (value.grouping === 'grouped' && count < 2)
  )
    throw new Error('Opções de envio inválidas.');
  if (value.presentation === 'media' && value.grouping === 'separate') return undefined;
  return Object.freeze({ presentation: value.presentation, grouping: value.grouping });
}
export type AttachmentIntent = Readonly<{
  mutationId: string;
  caption: string;
  replyTo: string | null;
  thread: boolean;
  options?: AttachmentSendOptions;
}>;
export type AttachmentBatchIntent = Readonly<{
  replyTo: string | null;
  thread: boolean;
  options?: AttachmentSendOptions;
}>;
export type AttachmentBatchFile = Readonly<{
  name: string;
  mime: string;
  bytes: Uint8Array;
  thumb?: Uint8Array | null;
  mutationId: string;
  caption: string;
}>;
