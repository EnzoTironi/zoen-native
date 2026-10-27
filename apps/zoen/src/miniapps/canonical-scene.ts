import {
  token,
  digest,
  integer,
  metadata,
  validateDoc,
  hash,
  decode64,
  encode64,
  decodeUtf8,
  utf8Length,
} from './canonical-scene-codec.ts';
export { hash, encode64, decode64 } from './canonical-scene-codec.ts';
const encoder = new TextEncoder();
import type { CallToolResult } from '@modelcontextprotocol/client';

export const CANONICAL_SCENE_CHUNK_BYTES = 196_608;
export const MAX_CANONICAL_SCENE_BYTES = 16 * 1024 * 1024;
export type CanonicalSceneDoc = Readonly<{
  item_id: string;
  version: number;
  sha256: string;
  scene_json: string;
  current_version: number;
}>;
export type CanonicalScenePhase = 'idle' | 'saving' | 'saved' | 'error' | 'closed';
export type CanonicalSceneSave = Readonly<{
  item_id: string;
  expected_version: number;
  expected_sha256: string;
  scene_json: string;
  mutation_id: string;
}>;
export type CanonicalSceneOptions = {
  initialDoc: CanonicalSceneDoc;
  check: () => void;
  id: () => string;
  save: (input: CanonicalSceneSave) => Promise<CanonicalSceneDoc>;
  onState?: (phase: CanonicalScenePhase) => void;
};

function result(value: Record<string, unknown>): CallToolResult {
  return { content: [{ type: 'text', text: JSON.stringify(value) }], structuredContent: value };
}
type Upload = {
  id: string;
  mutation: string;
  bytes: number;
  sha256: string;
  base: Pick<CanonicalSceneDoc, 'version' | 'sha256'>;
  buffer: Uint8Array | null;
  scene: string | null;
  received: number;
  chunks: string[];
  task: Promise<CallToolResult> | null;
  input: CanonicalSceneSave | null;
  failed: boolean;
  aborting: boolean;
};
type Completed = Pick<Upload, 'id' | 'mutation' | 'bytes' | 'sha256' | 'chunks'> & {
  task: Promise<CallToolResult>;
};

/** One frame, one exact Item, one bounded upload. Rust owns scene validation and durable CAS. */
export class CanonicalSceneHost {
  private readonly options: Omit<CanonicalSceneOptions, 'initialDoc'>;
  private active = true;
  private doc: CanonicalSceneDoc | null;
  private readonly item: string;
  private upload: Upload | null = null;
  private completed: Completed | null = null;
  private aborted: string | null = null;
  private read: { id: string; bytes: Uint8Array; version: number; sha256: string } | null = null;
  private sequence = 0;
  private refresh: { id: string; doc: CanonicalSceneDoc; base: CanonicalSceneDoc } | null = null;

  constructor(options: CanonicalSceneOptions) {
    this.options = {
      check: options.check,
      id: options.id,
      save: options.save,
      onState: options.onState,
    };
    this.doc = validateDoc(options.initialDoc);
    this.item = this.doc.item_id;
    this.check();
  }
  private check() {
    if (!this.active) throw new Error('This canonical scene has closed.');
    try {
      this.options.check();
    } catch (error) {
      this.dispose();
      throw error;
    }
  }
  private state(phase: CanonicalScenePhase) {
    this.check();
    try {
      this.options.onState?.(phase);
    } catch {
      /* Presentation cannot change a durable ACK. */
    }
  }
  private newId() {
    const id = this.options.id();
    if (!token(id) || id.length > 96) throw new Error('Invalid scene transfer identifier.');
    this.check();
    return `${id}-${++this.sequence}`;
  }
  call(name: string, args: Record<string, unknown>): Promise<CallToolResult> {
    try {
      this.check();
      if (!args || Array.isArray(args) || !Object.hasOwn(args, 'id') || args.id !== this.item)
        throw new Error('This scene does not belong to the current Item.');
      switch (name) {
        case 'app_scene_read':
          return Promise.resolve(this.readStart(args));
        case 'app_scene_read_chunk':
          return Promise.resolve(this.readChunk(args));
        case 'app_scene_begin':
          return Promise.resolve(this.begin(args));
        case 'app_scene_append':
          return Promise.resolve(this.append(args));
        case 'app_scene_commit':
          return this.commit(args);
        case 'app_scene_abort':
          return this.abort(args);
        default:
          throw new Error('This canonical scene tool is unavailable.');
      }
    } catch (error) {
      return Promise.reject(error);
    }
  }
  private readStart(args: Record<string, unknown>) {
    if (args.refreshId !== undefined && args.refreshId !== this.refresh?.id)
      throw new Error('This drawing refresh is no longer available.');
    const doc = args.refreshId ? this.refresh!.doc : this.doc!;
    if (!this.read || this.read.version !== doc.version || this.read.sha256 !== doc.sha256) {
      this.read?.bytes.fill(0);
      this.read = {
        id: this.newId(),
        bytes: encoder.encode(doc.scene_json),
        version: doc.version,
        sha256: doc.sha256,
      };
    }
    return result({
      transferId: this.read.id,
      bytes: this.read.bytes.length,
      sha256: this.read.sha256,
      chunkBytes: CANONICAL_SCENE_CHUNK_BYTES,
      version: this.read.version,
      currentVersion: doc.current_version,
    });
  }
  private readChunk(args: Record<string, unknown>) {
    const read = this.read;
    if (
      !read ||
      args.transferId !== read.id ||
      !integer(args.index) ||
      args.index < 0 ||
      args.index >= Math.ceil(read.bytes.length / CANONICAL_SCENE_CHUNK_BYTES)
    )
      throw new Error('Invalid scene read chunk.');
    const offset = args.index * CANONICAL_SCENE_CHUNK_BYTES;
    return result({
      index: args.index,
      data: encode64(read.bytes.subarray(offset, offset + CANONICAL_SCENE_CHUNK_BYTES)),
    });
  }
  private begin(args: Record<string, unknown>) {
    if (this.refresh) throw new Error('The drawing is applying a shared update.');
    if (
      !token(args.mutationId) ||
      !integer(args.bytes) ||
      args.bytes < 1 ||
      args.bytes > MAX_CANONICAL_SCENE_BYTES ||
      !digest(args.sha256)
    )
      throw new Error('Invalid or oversized scene upload.');
    const prior =
      this.upload ?? (this.completed?.mutation === args.mutationId ? this.completed : null);
    if (prior) {
      if ('aborting' in prior && prior.aborting)
        throw new Error('This scene upload is being aborted.');
      if (
        prior.mutation !== args.mutationId ||
        prior.bytes !== args.bytes ||
        prior.sha256 !== args.sha256
      )
        throw new Error(
          'A different scene upload is already pending. Abort it before replacing it.',
        );
      return result({ uploadId: prior.id, chunkBytes: CANONICAL_SCENE_CHUNK_BYTES });
    }
    const upload: Upload = {
      id: this.newId(),
      mutation: args.mutationId,
      bytes: args.bytes,
      sha256: args.sha256,
      base: Object.freeze({ version: this.doc!.version, sha256: this.doc!.sha256 }),
      buffer: new Uint8Array(args.bytes),
      scene: null,
      received: 0,
      chunks: [],
      task: null,
      input: null,
      failed: false,
      aborting: false,
    };
    this.upload = upload;
    return result({ uploadId: upload.id, chunkBytes: CANONICAL_SCENE_CHUNK_BYTES });
  }
  private append(args: Record<string, unknown>) {
    const upload =
      this.upload?.id === args.uploadId
        ? this.upload
        : this.completed?.id === args.uploadId
          ? this.completed
          : null;
    if (
      !upload ||
      ('aborting' in upload && upload.aborting) ||
      !integer(args.index) ||
      args.index < 0
    )
      throw new Error('Invalid scene upload chunk.');
    const index = args.index;
    const bytes = decode64(args.data);
    const expected = Math.min(
      CANONICAL_SCENE_CHUNK_BYTES,
      upload.bytes - index * CANONICAL_SCENE_CHUNK_BYTES,
    );
    if (expected <= 0 || bytes.length !== expected)
      throw new Error('Invalid scene upload chunk size.');
    const sha = hash(bytes);
    if (index < upload.chunks.length) {
      if (upload.chunks[index] !== sha)
        throw new Error('A repeated scene chunk changed its bytes.');
      return result({ nextIndex: index + 1 });
    }
    if (index !== upload.chunks.length || !('buffer' in upload) || !upload.buffer || upload.task)
      throw new Error('Scene upload chunks must arrive in order.');
    upload.buffer.set(bytes, upload.received);
    upload.received += bytes.length;
    upload.chunks.push(sha);
    return result({ nextIndex: upload.chunks.length });
  }
  private commit(args: Record<string, unknown>): Promise<CallToolResult> {
    const completed = this.completed;
    if (completed && completed.id === args.uploadId) return completed.task;
    const upload = this.upload;
    if (!upload || args.uploadId !== upload.id) throw new Error('Invalid scene upload.');
    if (upload.aborting && !upload.task) throw new Error('This scene upload is being aborted.');
    if (upload.task) return upload.task;
    const scene = this.uploadScene(upload);
    const input: CanonicalSceneSave = (upload.input ??= Object.freeze({
      item_id: this.item,
      expected_version: upload.base.version,
      expected_sha256: upload.base.sha256,
      scene_json: scene,
      mutation_id: upload.mutation,
    }));
    upload.failed = false;
    const task = Promise.resolve()
      .then(async () => {
        this.state('saving');
        this.check();
        const accepted = await this.options.save(input);
        this.check();
        metadata(accepted);
        if (
          accepted.item_id !== this.item ||
          accepted.sha256 !== upload.sha256 ||
          accepted.scene_json !== input.scene_json ||
          accepted.version < upload.base.version ||
          (accepted.version === upload.base.version && accepted.sha256 !== upload.base.sha256)
        )
          throw new Error('The save did not acknowledge this exact scene.');
        this.doc = Object.freeze({ ...accepted, scene_json: input.scene_json });
        this.read?.bytes.fill(0);
        this.read = null;
        this.completed = {
          id: upload.id,
          mutation: upload.mutation,
          bytes: upload.bytes,
          sha256: upload.sha256,
          chunks: upload.chunks,
          task,
        };
        if (accepted.current_version > accepted.version) {
          // Our save is durable, but a peer already advanced the Item. Keep the
          // submitted scene available for an explicit native Save Copy action.
          upload.failed = true;
          this.state('error');
        } else {
          if (this.upload === upload) this.upload = null;
          upload.scene = null;
          upload.input = null;
          this.state('saved');
        }
        this.check();
        return result({
          checkpointId: this.item,
          version: accepted.version,
          sha256: accepted.sha256,
          currentVersion: accepted.current_version,
        });
      })
      .catch((error: unknown) => {
        if (this.upload === upload) {
          upload.task = null;
          upload.failed = true;
        }
        if (this.active) {
          try {
            this.state('error');
          } catch {
            /* A retired owner cannot publish an error. */
          }
        }
        throw error;
      });
    upload.task = task;
    return task;
  }
  private uploadScene(upload: Upload): string {
    if (upload.received !== upload.bytes) throw new Error('The scene upload is incomplete.');
    if (!upload.scene) {
      if (!upload.buffer || hash(upload.buffer) !== upload.sha256)
        throw new Error('The uploaded scene hash does not match its bytes.');
      const scene = decodeUtf8(upload.buffer);
      if (utf8Length(scene) !== upload.bytes) throw new Error('Invalid scene UTF-8.');
      upload.scene = scene;
      upload.buffer.fill(0);
      upload.buffer = null;
    }
    return upload.scene;
  }
  private async abort(args: Record<string, unknown>) {
    if (!token(args.uploadId)) throw new Error('Invalid scene upload.');
    const upload = this.upload;
    if (!upload || args.uploadId !== upload.id) {
      if (args.uploadId === this.aborted || args.uploadId === this.completed?.id) return result({});
      throw new Error('Invalid scene upload.');
    }
    // A commit already admitted to Rust may have succeeded. Drain it before releasing its bytes.
    upload.aborting = true;
    if (upload.task) await upload.task.catch(() => {});
    this.check();
    const wasCurrent = this.upload === upload;
    if (wasCurrent) this.upload = null;
    upload.buffer?.fill(0);
    upload.buffer = null;
    upload.scene = null;
    upload.input = null;
    this.aborted = upload.id;
    if (wasCurrent) this.state('idle');
    return result({});
  }
  /** Native-only explicit Save Copy affordance; never exposed as a guest tool. */
  failedScene(): Readonly<{ scene_json: string; mutation_id: string }> | null {
    if (!this.active) return null;
    this.check();
    const upload = this.upload;
    return upload?.failed && !upload.aborting && upload.scene !== null
      ? Object.freeze({ scene_json: upload.scene, mutation_id: upload.mutation })
      : null;
  }
  /** Native-initiated copy preparation only; this accessor never commits the original Item. */
  stagedScene(
    uploadId: string,
    bytes: number,
    expectedSha: string,
  ): Readonly<{ scene_json: string; mutation_id: string }> {
    this.check();
    const upload = this.upload;
    if (
      !upload ||
      upload.id !== uploadId ||
      upload.bytes !== bytes ||
      upload.sha256 !== expectedSha ||
      upload.task ||
      upload.aborting
    )
      throw new Error('The prepared scene does not match this upload.');
    const scene = this.uploadScene(upload);
    this.check();
    return Object.freeze({ scene_json: scene, mutation_id: upload.mutation });
  }
  head(): CanonicalSceneDoc {
    this.check();
    return this.doc!;
  }
  /** The accepted CAS base stays unchanged until this frame acknowledges canvas application. */
  stageRefresh(doc: CanonicalSceneDoc): string {
    this.check();
    if (this.refresh || this.upload) throw new Error('The drawing has pending changes.');
    const candidate = validateDoc(doc);
    if (
      candidate.item_id !== this.item ||
      candidate.version !== candidate.current_version ||
      candidate.version <= this.doc!.version
    )
      throw new Error('Invalid drawing refresh head.');
    const id = this.newId();
    this.refresh = { id, doc: candidate, base: this.doc! };
    return id;
  }
  acceptRefresh(id: string) {
    this.check();
    const refresh = this.refresh;
    if (!refresh || refresh.id !== id || refresh.base !== this.doc || this.upload)
      throw new Error('The drawing changed while applying its update.');
    this.doc = refresh.doc;
    this.cancelRefresh(id);
  }
  cancelRefresh(id: string) {
    this.check();
    if (this.refresh?.id !== id) return;
    this.refresh = null;
    this.read?.bytes.fill(0);
    this.read = null;
  }
  dispose() {
    if (!this.active) return;
    this.active = false;
    this.read?.bytes.fill(0);
    this.upload?.buffer?.fill(0);
    if (this.upload) {
      this.upload.buffer = null;
      this.upload.scene = null;
      this.upload.input = null;
    }
    this.upload = null;
    this.read = null;
    this.completed = null;
    this.doc = null;
    this.refresh = null;
    try {
      this.options.check();
      this.options.onState?.('closed');
    } catch {
      /* Old owner stays silent. */
    }
  }
}
