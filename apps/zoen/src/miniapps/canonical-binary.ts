import type { CallToolResult } from '@modelcontextprotocol/client';
import type { AppBinaryDocumentDto } from '../core/generated/AppBinaryDocumentDto';
import { binaryEditorFormat, type BinaryEditorId } from './binary-formats.ts';
export { DECKCRAFT_ENGINE_REVISION } from './binary-formats.ts';
import {
  CANONICAL_SCENE_CHUNK_BYTES as CHUNK,
  decode64,
  encode64,
  hash,
} from './canonical-scene.ts';

export const MAX_BINARY_DOCUMENT_BYTES = 8 * 1024 * 1024;
export const canonicalBinaryTools = new Set([
  'app_binary_read',
  'app_binary_read_chunk',
  'app_binary_begin',
  'app_binary_append',
  'app_binary_commit',
  'app_binary_abort',
  'app_binary_export_source',
]);
export type BinarySave = Readonly<{
  item_id: string;
  expected_version: number;
  expected_sha256: string;
  data_base64: string;
  mutation_id: string;
  intent_sha256: string;
}>;
type Options = {
  editor?: BinaryEditorId;
  itemId: string;
  check: () => void;
  id: () => string;
  read: (version?: number, sha256?: string) => Promise<AppBinaryDocumentDto>;
  save: (input: BinarySave) => Promise<AppBinaryDocumentDto>;
};
const token = (v: unknown): v is string =>
  typeof v === 'string' && /^[A-Za-z0-9_-]{1,128}$/.test(v);
const digest = (v: unknown): v is string => typeof v === 'string' && /^[a-f0-9]{64}$/.test(v);
const version = (v: unknown): v is number =>
  typeof v === 'number' && Number.isSafeInteger(v) && v >= 1 && v <= 0xffffffff;
const integer = (v: unknown): v is number =>
  typeof v === 'number' && Number.isSafeInteger(v) && v >= 0;
function keys(args: Record<string, unknown>, names: string[]) {
  if (Object.keys(args).some((k) => !names.includes(k)))
    throw new Error('Unexpected App argument.');
}
function result(value: Record<string, unknown>): CallToolResult {
  return { content: [{ type: 'text', text: JSON.stringify(value) }], structuredContent: value };
}
function encodeDocument(bytes: Uint8Array) {
  const chunks: string[] = [];
  for (let i = 0; i < bytes.length; i += CHUNK) chunks.push(encode64(bytes.subarray(i, i + CHUNK)));
  return chunks.join('');
}
function validated(doc: AppBinaryDocumentDto, item: string, editor: BinaryEditorId) {
  const format = binaryEditorFormat(editor);
  if (
    doc.item_id !== item ||
    doc.kind !== format.id ||
    !version(doc.version) ||
    !version(doc.current_version) ||
    doc.current_version < doc.version ||
    !digest(doc.sha256) ||
    !token(doc.author_id) ||
    !token(doc.actor_id) ||
    !['human', 'agent'].includes(doc.actor_kind) ||
    doc.engine_revision !== format.engineRevision ||
    !integer(doc.byte_length) ||
    doc.byte_length < 1 ||
    doc.byte_length > format.maxDocumentBytes ||
    typeof doc.data_base64 !== 'string' ||
    doc.data_base64.length !== Math.ceil(doc.byte_length / 3) * 4
  )
    throw new Error('Invalid canonical App document.');
  const bytes = new Uint8Array(doc.byte_length);
  const encodedChunk = (CHUNK / 3) * 4;
  let offset = 0;
  for (let i = 0; i < doc.data_base64.length; i += encodedChunk) {
    const chunk = decode64(doc.data_base64.slice(i, i + encodedChunk));
    if (chunk.length !== Math.min(CHUNK, bytes.length - offset))
      throw new Error('Invalid canonical App bytes.');
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  if (hash(bytes) !== doc.sha256) {
    bytes.fill(0);
    throw new Error('The canonical App hash does not match.');
  }
  return bytes;
}
type Upload = {
  id: string;
  mutation: string;
  base: number;
  baseHash: string;
  intent: string;
  size: number;
  sha: string;
  buffer: Uint8Array | null;
  received: number;
  chunks: string[];
  input: BinarySave | null;
  task: Promise<CallToolResult> | null;
  aborting: boolean;
  attempted: boolean;
};
type Completed = Pick<
  Upload,
  'id' | 'mutation' | 'base' | 'baseHash' | 'intent' | 'size' | 'sha' | 'chunks'
> & { task: Promise<CallToolResult> };

/** Standard MCP tools over the existing native canonical Item API. All retained bytes
 * are bounded transfer buffers; only the Rust core accepts versions or stores content. */
export class CanonicalBinaryHost {
  private active = true;
  private sequence = 0;
  private readGeneration = 0;
  private upload: Upload | null = null;
  private completed: Completed | null = null;
  private aborted: string | null = null;
  private read: { id: string; bytes: Uint8Array; metadata: Record<string, unknown> } | null = null;
  private readonly options: Options;
  constructor(options: Options) {
    this.options = { ...options, editor: binaryEditorFormat(options.editor).id };
    if (!token(options.itemId)) throw new Error('Invalid App Item.');
    this.check();
  }
  private check() {
    if (!this.active) throw new Error('This App binding has closed.');
    try {
      this.options.check();
    } catch (error) {
      this.dispose();
      throw error;
    }
  }
  private id() {
    const id = this.options.id();
    if (!token(id) || id.length > 96) throw new Error('Invalid App transfer identifier.');
    this.check();
    return `${id}-${++this.sequence}`;
  }
  async call(name: string, args: Record<string, unknown>): Promise<CallToolResult> {
    this.check();
    if (
      !args ||
      Array.isArray(args) ||
      !Object.hasOwn(args, 'id') ||
      args.id !== this.options.itemId
    )
      throw new Error('This document does not belong to the current App.');
    switch (name) {
      case 'app_binary_read':
        return this.readStart(args, false);
      case 'app_binary_export_source':
        return this.readStart(args, true);
      case 'app_binary_read_chunk':
        return this.readChunk(args);
      case 'app_binary_begin':
        return this.begin(args);
      case 'app_binary_append':
        return this.append(args);
      case 'app_binary_commit':
        return this.commit(args);
      case 'app_binary_abort':
        return this.abort(args);
      default:
        throw new Error('This App tool is unavailable.');
    }
  }
  private async readStart(args: Record<string, unknown>, exact: boolean) {
    keys(args, ['id', 'version', 'sha256']);
    const selected = args.version !== undefined || args.sha256 !== undefined;
    if ((exact || selected) && (!version(args.version) || !digest(args.sha256)))
      throw new Error('An exact accepted version is required.');
    const generation = ++this.readGeneration;
    const doc = await this.options.read(
      selected ? (args.version as number) : undefined,
      selected ? (args.sha256 as string) : undefined,
    );
    this.check();
    if (generation !== this.readGeneration) throw new Error('This App read was replaced.');
    const bytes = validated(doc, this.options.itemId, this.options.editor ?? 'deckcraft');
    if (selected && (doc.version !== args.version || doc.sha256 !== args.sha256)) {
      bytes.fill(0);
      throw new Error('The selected App version changed.');
    }
    this.read?.bytes.fill(0);
    const metadata = {
      transferId: this.id(),
      bytes: bytes.length,
      sha256: doc.sha256,
      version: doc.version,
      currentVersion: doc.current_version,
      chunkBytes: CHUNK,
      engineRevision: doc.engine_revision,
      authorId: doc.author_id,
      scope: { artifactId: doc.item_id, actorId: doc.actor_id, actorKind: doc.actor_kind },
    };
    this.read = { id: metadata.transferId, bytes, metadata };
    return result(metadata);
  }
  private readChunk(args: Record<string, unknown>) {
    keys(args, ['id', 'transferId', 'index']);
    const read = this.read;
    if (
      !read ||
      args.transferId !== read.id ||
      !integer(args.index) ||
      args.index >= Math.ceil(read.bytes.length / CHUNK)
    )
      throw new Error('Invalid App read chunk.');
    return result({
      index: args.index,
      data: encode64(read.bytes.subarray(args.index * CHUNK, (args.index + 1) * CHUNK)),
    });
  }
  private begin(args: Record<string, unknown>) {
    keys(args, [
      'id',
      'mutationId',
      'version',
      'sha256',
      'bytes',
      'documentSha256',
      'intentSha256',
    ]);
    if (
      !token(args.mutationId) ||
      !version(args.version) ||
      args.version === 0xffffffff ||
      !digest(args.sha256) ||
      !integer(args.bytes) ||
      args.bytes < 1 ||
      args.bytes > MAX_BINARY_DOCUMENT_BYTES ||
      !digest(args.documentSha256) ||
      !digest(args.intentSha256)
    )
      throw new Error('Invalid App proposal.');
    const prior =
      this.upload ?? (this.completed?.mutation === args.mutationId ? this.completed : null);
    if (prior) {
      if (
        ('aborting' in prior && prior.aborting) ||
        prior.mutation !== args.mutationId ||
        prior.base !== args.version ||
        prior.baseHash !== args.sha256 ||
        prior.intent !== args.intentSha256 ||
        prior.size !== args.bytes ||
        prior.sha !== args.documentSha256
      )
        throw new Error('A different App proposal is pending.');
      return result({ uploadId: prior.id, chunkBytes: CHUNK });
    }
    this.upload = {
      id: this.id(),
      mutation: args.mutationId,
      base: args.version,
      baseHash: args.sha256,
      intent: args.intentSha256,
      size: args.bytes,
      sha: args.documentSha256,
      buffer: new Uint8Array(args.bytes),
      received: 0,
      chunks: [],
      input: null,
      task: null,
      aborting: false,
      attempted: false,
    };
    return result({ uploadId: this.upload.id, chunkBytes: CHUNK });
  }
  private append(args: Record<string, unknown>) {
    keys(args, ['id', 'uploadId', 'index', 'data']);
    const upload =
      this.upload?.id === args.uploadId
        ? this.upload
        : this.completed?.id === args.uploadId
          ? this.completed
          : null;
    if (!upload || ('aborting' in upload && upload.aborting) || !integer(args.index))
      throw new Error('Invalid App proposal chunk.');
    const bytes = decode64(args.data);
    const expected = Math.min(CHUNK, upload.size - args.index * CHUNK);
    if (expected <= 0 || bytes.length !== expected) throw new Error('Invalid App chunk size.');
    const sha = hash(bytes);
    if (args.index < upload.chunks.length) {
      if (upload.chunks[args.index] !== sha) throw new Error('A repeated App chunk changed.');
      return result({ nextIndex: args.index + 1 });
    }
    if (
      args.index !== upload.chunks.length ||
      !('buffer' in upload) ||
      !upload.buffer ||
      upload.task
    )
      throw new Error('App chunks must arrive in order.');
    upload.buffer.set(bytes, upload.received);
    upload.received += bytes.length;
    upload.chunks.push(sha);
    return result({ nextIndex: upload.chunks.length });
  }
  private commit(args: Record<string, unknown>): Promise<CallToolResult> {
    keys(args, ['id', 'uploadId']);
    if (!token(args.uploadId)) throw new Error('Invalid App proposal identifier.');
    const completed = this.completed;
    if (completed && completed.id === args.uploadId) return completed.task;
    const upload = this.upload;
    if (!upload || upload.id !== args.uploadId || (upload.aborting && !upload.task))
      throw new Error('Invalid App proposal.');
    if (upload.task) return upload.task;
    if (!upload.input) {
      if (!upload.buffer || upload.received !== upload.size || hash(upload.buffer) !== upload.sha)
        throw new Error('The App proposal is incomplete or has changed.');
      upload.input = Object.freeze({
        item_id: this.options.itemId,
        expected_version: upload.base,
        expected_sha256: upload.baseHash,
        data_base64: encodeDocument(upload.buffer),
        mutation_id: upload.mutation,
        intent_sha256: upload.intent,
      });
      upload.buffer.fill(0);
      upload.buffer = null;
    }
    const input = upload.input;
    const task = Promise.resolve()
      .then(async () => {
        this.check();
        upload.attempted = true;
        const accepted = await this.options.save(input);
        this.check();
        const verified = validated(
          accepted,
          this.options.itemId,
          this.options.editor ?? 'deckcraft',
        );
        verified.fill(0);
        if (
          accepted.version !== upload.base + 1 ||
          accepted.sha256 !== upload.sha ||
          accepted.data_base64 !== input.data_base64
        )
          throw new Error('The save did not acknowledge this exact App proposal.');
        this.readGeneration++;
        this.read?.bytes.fill(0);
        this.read = null;
        this.completed = {
          id: upload.id,
          mutation: upload.mutation,
          base: upload.base,
          baseHash: upload.baseHash,
          intent: upload.intent,
          size: upload.size,
          sha: upload.sha,
          chunks: upload.chunks,
          task,
        };
        if (this.upload === upload) this.upload = null;
        upload.input = null;
        return result({
          mutationId: upload.mutation,
          head: { version: String(accepted.version), sha256: accepted.sha256 },
          currentVersion: accepted.current_version,
          superseded: accepted.current_version > accepted.version,
          authorId: accepted.author_id,
          engineRevision: accepted.engine_revision,
        });
      })
      .catch((error: unknown) => {
        if (this.upload === upload) upload.task = null;
        throw error;
      });
    upload.task = task;
    return task;
  }
  private async abort(args: Record<string, unknown>) {
    keys(args, ['id', 'uploadId']);
    if (!token(args.uploadId)) throw new Error('Invalid App proposal.');
    if (this.aborted === args.uploadId)
      return result({ aborted: true, outcome: 'locallyCancelled' });
    if (this.completed?.id === args.uploadId)
      return result({ aborted: false, committed: true, outcome: 'committed' });
    const upload = this.upload;
    if (!upload || upload.id !== args.uploadId) throw new Error('Invalid App proposal.');
    upload.aborting = true;
    if (upload.task) {
      try {
        await upload.task;
      } catch {
        /* A failed write preserves the immutable intent until explicitly aborted. */
      }
    }
    this.check();
    if (this.completed?.id === args.uploadId)
      return result({ aborted: false, committed: true, outcome: 'committed' });
    if (upload.attempted) {
      upload.aborting = false;
      return result({
        aborted: false,
        outcome: 'unknown',
        uploadId: upload.id,
        mutationId: upload.mutation,
      });
    }
    upload.buffer?.fill(0);
    upload.buffer = null;
    upload.input = null;
    if (this.upload === upload) this.upload = null;
    this.aborted = upload.id;
    return result({ aborted: true, outcome: 'locallyCancelled' });
  }
  dispose() {
    if (!this.active) return;
    this.active = false;
    this.readGeneration++;
    this.read?.bytes.fill(0);
    this.read = null;
    if (this.upload) {
      this.upload.buffer?.fill(0);
      this.upload.buffer = null;
      this.upload.input = null;
    }
    this.upload = null;
    this.completed = null;
  }
}
