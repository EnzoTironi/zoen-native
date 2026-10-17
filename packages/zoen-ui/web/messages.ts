import { array, boolean, bytes, hex, integer, message, object, publicKey, string } from './boundary.ts';
import { persistBeforeOutgoing } from './persistence.ts';
import type { StoredSession } from './session.ts';
import { AuthenticatedTransport, type ConnectionState, type RelayFrame } from './transport.ts';
import type { BrowserVault } from './vault.ts';
import type { CoreWorker } from './worker-client.ts';
import type { PageDto, PageSaveRequest } from './live-page.tsx';
import { parsePage, parsePageRequest } from './page-data.ts';
import { download, prepare, parsePlan, parseManifest, upload, ensurePage, MAX_FILE_BYTES as MAX_PREPARED_FILE_BYTES, type FileCore, type PreparedFile } from './file-transfer.ts';

function nullableText(value: unknown): string | null { return value === null ? null : string(value); }
function timestamp(value: unknown): number {
  if (typeof value !== 'number' || !Number.isSafeInteger(value)) throw new Error('Invalid message timestamp.');
  return value;
}
function parseMessage(input: unknown) {
  const value = object(input);
  const delivery = string(value.delivery);
  if (delivery !== 'confirmed' && delivery !== 'sending' && delivery !== 'failed') throw new Error('Invalid message delivery state.');
  const ticks = value.ticks === null || value.ticks === undefined ? null : string(value.ticks);
  if (ticks !== null && ticks !== 'sent' && ticks !== 'delivered' && ticks !== 'read') throw new Error('Invalid message receipt.');
  const quote = value.quote ? object(value.quote) : null;
  let text = string(value.text);
  let file: FileDoc | null = null;
  // A bad attachment must not prevent saving or restoring the conversation's sync state.
  if (value.file) {
    try { file = parseFile(value.file); }
    catch { text = 'Anexo indisponível'; }
  }
  return { id: string(value.id), entry: value.entry ? string(value.entry) : null, author: publicKey(value.author), name: string(value.name), text,
    at_ms: timestamp(value.at_ms), delivery, failure: nullableText(value.failure), ticks: ticks as 'sent' | 'delivered' | 'read' | null,
    edited: value.edited === true, deleted: value.deleted === true, forwarded: value.forwarded === true,
    reactions: array(value.reactions ?? []).map((r) => { const x = object(r); return {
      emoji: string(x.emoji), count: integer(x.count), mine: x.mine === true,
      participants: array(x.participants ?? []).map((p) => { const participant = object(p); return { identity: publicKey(participant.identity), name: string(participant.name) }; }),
    }; }),
    quote: quote ? { id: string(quote.id), name: string(quote.name), text: string(quote.text) } : null,
    thread: value.thread ? string(value.thread) : null, replies: integer(value.replies ?? 0),
    repliers: array(value.repliers ?? []).map(string), last_reply_ms: typeof value.last_reply_ms === 'number' ? value.last_reply_ms : null,
    file };
}

const HASH = /^[0-9a-f]{64}$/;
function parseChunk(input: unknown) {
  const v = object(input);
  const chunk = { sha256: string(v.sha256), bytes: integer(v.bytes), blob: string(v.blob), key: string(v.key) };
  if (![chunk.sha256, chunk.blob, chunk.key].every((h) => HASH.test(h))) throw new Error('Invalid file chunk.');
  return chunk;
}
function parseVoice(input: unknown) {
  const v = object(input);
  const reply = v.reply ? object(v.reply) : null;
  return { ms: integer(v.ms), levels: string(v.levels ?? ''), transcript: string(v.transcript ?? ''),
    reply: reply ? { to: string(reply.to), thread: boolean(reply.thread ?? false) } : null };
}
function parseFile(input: unknown) {
  const v = object(input);
  return { name: string(v.name), path: string(v.path ?? ''), mime: string(v.mime), bytes: integer(v.bytes), sha256: string(v.sha256),
    chunks: array(v.chunks).map(parseChunk), thumb: v.thumb ? parseChunk(v.thumb) : null,
    voice: v.voice ? parseVoice(v.voice) : null };
}
export type FileDoc = ReturnType<typeof parseFile>;
export type Chunk = ReturnType<typeof parseChunk>;
export type VoiceMeta = ReturnType<typeof parseVoice>;

// Keep received media within the same bounds as roda-ffi/src/files.rs.
const MAX_FILE_BYTES = 512 << 20;
const MAX_CHUNK_BYTES = 16 << 20;
const MAX_THUMBNAIL_BYTES = 2 << 20;
function boundedFile(input: FileDoc): FileDoc {
  const invalid = () => new Error('Arquivo inválido ou grande demais para abrir.');
  if (!input.chunks.length || input.chunks.length > MAX_FILE_BYTES / (1 << 20) + 1) throw invalid();
  const file = parseFile(input); // Snapshot the manifest before any asynchronous work.
  if (!file.bytes || file.bytes > MAX_FILE_BYTES || !HASH.test(file.sha256)) throw invalid();
  if (file.chunks.some((c) => !c.bytes || c.bytes > MAX_CHUNK_BYTES) ||
      file.chunks.reduce((total, c) => total + c.bytes, 0) !== file.bytes ||
      (file.thumb && (!file.thumb.bytes || file.thumb.bytes > MAX_THUMBNAIL_BYTES))) throw invalid();
  const metadata = new TextEncoder().encode(file.name + file.mime + file.sha256).length +
    (file.chunks.length + Number(!!file.thumb)) * (3 * 64 + 8);
  if (metadata > 128 << 10) throw invalid();
  return file;
}

function parseConversation(input: unknown) {
  const value = object(input);
  const kind = string(value.kind);
  if (kind !== 'direct' && kind !== 'group' && kind !== 'community' && kind !== 'personal') throw new Error('Invalid conversation kind.');
  return { id: string(value.id), title: string(value.title), kind, internal: value.internal === true, can_send: boolean(value.can_send),
    waiting: nullableText(value.waiting), unreadable: integer(value.unreadable),
    unread: integer(value.unread ?? 0), mentions: integer(value.mentions ?? 0), members: integer(value.members ?? 0), messages: array(value.messages).map(parseMessage) };
}
export type Conversation = ReturnType<typeof parseConversation>;
export type Message = ReturnType<typeof parseMessage>;

export function parseInvite(input: unknown) {
  const value = object(input);
  if (value.kind !== 'preview') throw new Error('The relay did not return an invitation.');
  const role = string(value.role);
  if (role !== 'Owner' && role !== 'Admin' && role !== 'Member' && role !== 'Reader') throw new Error('Invalid invitation role.');
  return { space: string(value.space), title: string(value.title), role, members: integer(value.members) };
}
export type Invite = ReturnType<typeof parseInvite>;

function parseUpdate(json: string) {
  const value = object(JSON.parse(json));
  return { snapshot: string(value.snapshot), frames: array(value.frames).map(bytes),
    conversations: array(value.conversations).map(parseConversation) };
}

/** Core refusals arrive in English; people see plain Portuguese. */
function plain(reason: string): string {
  if (/no longer available/i.test(reason)) return 'Essa mensagem não está mais disponível.';
  if (/foi apagada/i.test(reason)) return 'Essa mensagem foi apagada.';
  if (/Only the person/i.test(reason)) return 'Só quem enviou pode mudar esta mensagem.';
  if (/32 KB/i.test(reason)) return 'A mensagem não pode ficar vazia nem ser longa demais.';
  return 'Não deu para fazer isso agora. Tente de novo.';
}

export type MessagesState =
  | { kind: 'loading' }
  | { kind: 'active'; connection: ConnectionState; conversations: Conversation[]; notice: string | null }
  | { kind: 'failed'; message: string; conversations: Conversation[] };
type LinkedSession = Extract<StoredSession, { kind: 'linked' }>;
type MessageCore = Pick<CoreWorker, 'text' | 'bytes' | 'close'>;
type MessageVault = Pick<BrowserVault, 'save' | 'sign'>;

export class Messages {
  private readonly core: MessageCore;
  private readonly vault: MessageVault;
  private readonly session: LinkedSession;
  private readonly changed: (state: MessagesState) => void;
  private readonly transport: AuthenticatedTransport;
  private serial: Promise<void> = Promise.resolve();
  private closed = false;
  private connection: ConnectionState = { kind: 'connecting' };
  private generation = 0;
  private conversations: Conversation[] = [];
  private notice: string | null = null;
  private readonly pendingSpaceCommands = new Map<string, { editId: string; now: number }>();
  private readonly pendingFiles = new Map<string, PreparedFile>();
  private currentSnapshot: string;
  private readonly fileCache = new Map<string, Uint8Array>();

  constructor({ core, vault, session, changed }: {
    core: MessageCore; vault: MessageVault; session: LinkedSession; changed: (state: MessagesState) => void;
  }) {
    this.core = core; this.vault = vault; this.session = session; this.changed = changed;
    this.currentSnapshot = session.snapshot;
    for (const saved of session.pendingUploads ?? []) { const plan = parsePlan(saved.planJSON, saved.editId); this.pendingFiles.set(plan.editId, plan); }
    this.transport = new AuthenticatedTransport(core, vault, session.account, (connection) => {
      if (this.closed) return;
      this.connection = connection;
      if (connection.kind !== 'ready') ++this.generation;
      if (connection.kind === 'failed') { this.fail(connection.message); return; }
      this.publish();
    }, (frame, raw) => this.received(frame, raw));
  }

  async readPage(item: string): Promise<PageDto> {
    if (this.closed) throw Error('Reload Zoen to reopen this browser session.');
    await ensurePage({ pageDownloadManifest: (item) => this.core.text({ kind: 'page_download_manifest', item }), pageBlobSupply: async (item, blob, data) => { await this.core.text({ kind: 'page_blob_supply', item, blob, data }); } }, item, { relay: this.session.account.relay });
    const result = await this.core.text({ kind: 'page', item });
    if (this.closed) throw Error('This browser session has closed.');
    const page = parsePage(JSON.parse(result));
    const local = this.session.pageDrafts?.[item];
    return local ? { ...page, unsaved: true, save_error: local.error || 'Unsent edits', draft: parsePageRequest(JSON.parse(local.request)) } : page;
  }

  async savePage(request: PageSaveRequest, space: string): Promise<PageDto> {
    const raw = JSON.stringify(request);
    this.session.pageDrafts ??= {};
    this.session.pageDrafts[request.item_id] = { request: raw, error: 'Unsent edits' };
    await this.vault.save(this.stored());
    try {
      await this.act(space, { kind: 'page_apply', request: raw, now: Date.now() });
      delete this.session.pageDrafts[request.item_id];
      await this.vault.save(this.stored());
      return await this.readPage(request.item_id);
    } catch (error) {
      this.session.pageDrafts[request.item_id] = { request: raw, error: message(error) };
      await this.vault.save(this.stored());
      throw error;
    }
  }

  async createPage(space: string, title: string): Promise<PageDto> {
    if (this.closed || this.connection.kind !== 'ready') throw Error('Reconnect before creating a page.');
    const key = `page:${space}:${title}`;
    const mutation = this.pendingSpaceCommands.get(key) ?? { editId: crypto.randomUUID(), now: Date.now() };
    this.pendingSpaceCommands.set(key, mutation); const generation = this.generation;
    let page: PageDto | undefined;
    await this.enqueue(async () => {
      const envelope = object(JSON.parse(await this.core.text({ kind: 'page_create', space, title, now: mutation.now, editId: mutation.editId })));
      const result = parsePage(envelope.result);
      await this.apply(JSON.stringify(envelope.update), generation); page = result; this.pendingSpaceCommands.delete(key);
    }, () => generation !== this.generation || this.connection.kind !== 'ready' ? 'Reconnect before creating a page.' : null, false);
    if (!page) throw Error('The page could not be created.');
    return page;
  }

  /** Verified signed parent projection; unsupported browser writes are never simulated. */
  async spaceHome(space: string): Promise<Record<string, unknown>> {
    if (this.closed) throw Error('This browser session has closed.');
    const result = await this.core.text({ kind: 'living_space_home', space });
    if (this.closed) throw Error('This browser session has closed.');
    return object(JSON.parse(result));
  }

  /** Mutations share the durable outbox boundary; retries reuse their edit identity until persisted. */
  async spaceCommand(space: string, command: object): Promise<unknown> {
    if (this.closed) throw Error('Reload Zoen to reopen this browser session.');
    if (this.connection.kind !== 'ready') throw Error('Reconnect before updating this Space.');
    const json = JSON.stringify(command);
    const key = `${space}:${json}`;
    const mutation = this.pendingSpaceCommands.get(key) ?? { editId: crypto.randomUUID(), now: Date.now() };
    this.pendingSpaceCommands.set(key, mutation);
    const generation = this.generation;
    let result: unknown;
    await this.enqueue(async () => {
      const envelope = object(JSON.parse(await this.core.text({ kind: 'living_space_mutate', space, command: json, now: mutation.now, editId: mutation.editId })));
      await this.apply(JSON.stringify(envelope.update), generation);
      result = envelope.result;
      if (!(typeof result === 'object' && result !== null && 'pending' in result && result.pending === true)) this.pendingSpaceCommands.delete(key);
    }, () => generation !== this.generation || this.connection.kind !== 'ready' ? 'Reconnect before updating this Space.' : null, false);
    return result;
  }

  private fileCore(): FileCore {
    return {
      fileManifest: (item) => this.core.text({ kind: 'file_manifest', item }),
      fileOpen: (item, data) => this.core.bytes({ kind: 'file_open', item, data }),
      filePrepare: (space, name, mime, data, now) => this.core.text({ kind: 'file_prepare', space, name, mime, data, now }),
      fileUploadHeaders: (blob, now) => this.core.text({ kind: 'file_upload_headers', blob, now }),
      fileCommit: (space, plan, now, editId) => this.core.text({ kind: 'file_commit', space, plan, now, editId }),
    };
  }

  private stored(snapshot = this.currentSnapshot): LinkedSession {
    const pendingUploads = [...this.pendingFiles.values()].map(({ planJSON, editId }) => ({ planJSON, editId }));
    return { kind: 'linked', account: this.session.account, snapshot, ...(this.session.pageDrafts ? { pageDrafts: this.session.pageDrafts } : {}), ...(pendingUploads.length ? { pendingUploads } : {}) };
  }

  get pendingUploadCount(): number { return this.pendingFiles.size; }

  async readFile(item: string): Promise<{ name: string; mime: string; bytes: Uint8Array }> {
    if (this.closed) throw Error('Reload Zoen to reopen this browser session.');
    // Re-authorize every read before consulting plaintext already authenticated in this session.
    const raw = await this.core.text({ kind: 'file_manifest', item });
    const metadata = object(JSON.parse(raw)); const digest = string(metadata.sha256);
    const manifest = parseManifest(raw);
    if (manifest.item !== item || !/^[0-9a-f]{64}$/i.test(digest)) throw Error('Invalid file metadata.');
    const key = `${item}:${digest}`; const cached = this.fileCache.get(key);
    if (cached && cached.byteLength === manifest.bytes) return { name: manifest.name, mime: manifest.mime, bytes: cached.slice() };
    const core = this.fileCore();
    const file = await download({ ...core, fileManifest: async () => raw }, item, { relay: this.session.account.relay });
    if (this.closed) throw Error('This browser session has closed.');
    this.fileCache.set(key, file.bytes.slice());
    let cachedBytes = [...this.fileCache.values()].reduce((n, bytes) => n + bytes.byteLength, 0);
    while (cachedBytes > 128 * 1024 * 1024) { const oldest = this.fileCache.keys().next().value; if (!oldest) break; cachedBytes -= this.fileCache.get(oldest)!.byteLength; this.fileCache.delete(oldest); }
    return file;
  }

  private async finishFile(plan: PreparedFile, generation: number): Promise<void> {
    await upload(this.fileCore(), plan, { relay: this.session.account.relay });
    if (this.closed || generation !== this.generation || this.connection.kind !== 'ready') throw Error('Reconnect to finish this file upload.');
    const update = await this.fileCore().fileCommit(plan.plan.space, plan.planJSON, Date.now(), plan.editId);
    this.pendingFiles.delete(plan.editId);
    try { await this.apply(update, generation); }
    catch (error) { this.pendingFiles.set(plan.editId, plan); throw error; }
  }

  async uploadFile(space: string, file: File): Promise<string> {
    if (this.closed || this.connection.kind !== 'ready') throw Error('Reconnect before uploading files.');
    if (file.size > MAX_PREPARED_FILE_BYTES) throw Error('Choose a file up to 64 MiB.');
    const generation = this.generation;
    let item = '';
    await this.enqueue(async () => {
      const made = await prepare(this.fileCore(), space, file.name, file.type || 'application/octet-stream', new Uint8Array(await file.arrayBuffer()), Date.now());
      const existing = [...this.pendingFiles.values()].find((p) => p.plan.space === space && p.plan.doc.name === made.plan.doc.name && p.plan.doc.mime === made.plan.doc.mime && typeof p.plan.doc.sha256 === 'string' && p.plan.doc.sha256 === made.plan.doc.sha256);
      const plan = existing ?? made;
      if (!existing && this.pendingFiles.size >= 4) throw Error('Retry unfinished uploads before adding more files.');
      this.pendingFiles.set(plan.editId, plan);
      // The whole plan, including file keys, is encrypted by BrowserVault before any PUT.
      await this.vault.save(this.stored());
      this.notice = 'Uploading file…'; this.publish();
      await this.finishFile(plan, generation);
      item = plan.plan.item; this.notice = null; this.publish();
    }, () => generation !== this.generation || this.connection.kind !== 'ready' ? 'Reconnect before uploading files.' : null, false);
    return item;
  }

  retryUploads(): Promise<void> {
    const generation = this.generation;
    return this.enqueue(async () => { for (const plan of [...this.pendingFiles.values()]) await this.finishFile(plan, generation); this.notice = null; this.publish(); }, () => this.connection.kind !== 'ready' ? 'Reconnect before retrying uploads.' : null, false);
  }

  start(): Promise<void> {
    return this.enqueue(async () => {
      const json = await this.core.text({ kind: 'restore', account: JSON.stringify(this.session.account), snapshot: this.session.snapshot });
      this.conversations = array(JSON.parse(json)).map(parseConversation);
      if (this.closed) return;
      this.publish();
      this.transport.start();
    });
  }

  private publish(): void {
    if (!this.closed) this.changed({ kind: 'active', connection: this.connection, conversations: this.conversations, notice: this.notice });
  }

  private received(frame: RelayFrame, raw: Uint8Array): Promise<void> {
    const generation = this.generation;
    return this.enqueue(async () => {
      if (generation !== this.generation || this.connection.kind !== 'ready') return;
      const update = frame.kind === 'ready' ? await this.core.text({ kind: 'bootstrap' }) :
        await this.core.text({ kind: 'receive', bytes: raw, now: Date.now() });
      if (frame.kind === 'rejected' && frame.permanent) this.notice = frame.reason;
      await this.apply(update, generation);
      if (frame.kind === 'ready') {
        for (const plan of [...this.pendingFiles.values()]) {
          try { await this.finishFile(plan, generation); }
          catch { this.notice = 'A file upload is waiting. Retry it when connected.'; this.publish(); break; }
        }
      }
    });
  }

  private async apply(json: string, generation: number): Promise<void> {
    const update = parseUpdate(json);
    await persistBeforeOutgoing({ snapshot: this.stored(update.snapshot),
      save: async (snapshot) => { await this.vault.save(snapshot); this.currentSnapshot = snapshot.snapshot; }, frames: update.frames, emit: (frame) => {
        // A reconnect bootstraps again from the saved outbox; never emit stale frames.
        if (this.closed || this.connection.kind !== 'ready' || generation !== this.generation) return;
        this.transport.send(frame);
      } });
    if (this.closed) return;
    this.conversations = update.conversations;
    this.publish();
  }

  send({ space, text }: { space: string; text: string }): Promise<void> {
    if (this.closed) return Promise.reject(new Error('Reload Zoen to reopen this browser session.'));
    if (this.connection.kind !== 'ready') return Promise.reject(new Error('The relay is reconnecting. Try again when connected.'));
    if (!this.conversations.find((conversation) => conversation.id === space)?.can_send) {
      return Promise.reject(new Error('The encrypted conversation is not ready to send.'));
    }
    const generation = this.generation;
    return this.enqueue(async () => {
      this.notice = null;
      const update = await this.core.text({ kind: 'queue_message', space, text, now: Date.now() });
      await this.apply(update, generation);
    }, () => generation !== this.generation || this.connection.kind !== 'ready' ? 'The connection changed. Try again when connected.' : null);
  }

  /** Post with an optional inline reply, thread reply or forward. Refusals stay non-fatal. */
  post({ space, text, replyTo = '', thread = false, forwarded = false }: { space: string; text: string; replyTo?: string; thread?: boolean; forwarded?: boolean }): Promise<void> {
    return this.act(space, { kind: 'queue_post', space, text, replyTo, thread, forwarded, now: Date.now() });
  }

  /** Reaction, edit or delete for everyone, in the shared annotation shape. */
  annotate(space: string, target: string, note: { React: { emoji: string } } | 'Unreact' | { Edit: { text: string } } | 'Delete'): Promise<void> {
    return this.act(space, { kind: 'queue_annotation', space, target, note: JSON.stringify(note), now: Date.now() });
  }

  /** Seal like native, upload every encrypted chunk, then post the file into the chat. */
  async sendFile(space: string, file: { name: string; mime: string; bytes: Uint8Array; thumb?: Uint8Array | null; voice?: VoiceMeta }, progress?: (done: number, total: number) => void): Promise<void> {
    if (file.bytes.length > MAX_FILE_BYTES) throw new Error('Este arquivo é grande demais para o navegador.');
    let framed: Uint8Array;
    try { framed = await this.core.bytes({ kind: 'seal_file', name: file.name.replace(/[\\/\0]/g, '_').slice(0, 255) || 'arquivo', mime: file.mime || 'application/octet-stream', bytes: file.bytes, thumb: file.thumb ?? new Uint8Array() }); }
    catch { throw new Error('Não deu para preparar este arquivo.'); }
    const length = new DataView(framed.buffer, framed.byteOffset).getUint32(0, true);
    const head = object(JSON.parse(new TextDecoder().decode(framed.subarray(4, 4 + length))));
    const doc = parseFile(head.doc);
    if (file.voice) doc.voice = parseVoice(file.voice);
    const uploads = array(head.uploads).map((u) => { const x = object(u); return { blob: string(x.blob), len: integer(x.len) }; });
    let offset = 4 + length;
    try {
      for (const [i, upload] of uploads.entries()) {
        await this.upload(upload.blob, framed.subarray(offset, offset + upload.len));
        offset += upload.len;
        progress?.(i + 1, uploads.length);
      }
      // A disconnect after the last PUT still must retain the recording locally.
      await this.act(space, { kind: 'queue_file', space, doc: JSON.stringify(doc), now: Date.now() });
    } catch (error) {
      // Keep the sealed voice/file in the timeline as failed, like native — do not lose the clip.
      // Best effort: the upload error is what the user must see, even if queueing also fails.
      try {
        if (this.closed || !this.conversations.some((c) => c.id === space)) throw error;
        let start = 4 + length;
        const chunks: Record<string, number[]> = {};
        for (const upload of uploads) {
          chunks[upload.blob] = Array.from(framed.subarray(start, start + upload.len));
          start += upload.len;
        }
        const generation = this.generation;
        await this.enqueue(async () => {
          const update = await this.core.text({ kind: 'queue_failed_file', space, doc: JSON.stringify(doc), now: Date.now(),
            reason: error instanceof Error ? error.message : 'Sem conexão para enviar o arquivo. Tente de novo.', chunks: JSON.stringify(chunks) });
          await this.apply(update, generation);
        }, () => this.closed ? 'A sessão mudou.' : null, false);
      } catch { /* keep the original upload error */ }
      throw error;
    }
  }

  /** The same encrypted chunks, named again in another chat: nothing to upload. */
  forwardFile(space: string, doc: FileDoc): Promise<void> {
    return this.act(space, { kind: 'queue_file', space,
      doc: JSON.stringify({ ...doc, voice: doc.voice ? { ...doc.voice, reply: null } : null }), now: Date.now() });
  }

  private async upload(blob: string, sealed: Uint8Array): Promise<void> {
    for (let attempt = 0; ; attempt++) {
      const ts = Date.now();
      const payload = await this.core.bytes({ kind: 'blob_put_message', sha: blob, ts, relay: this.transport.relayName });
      const sig = Array.from(await this.vault.sign(payload), (b) => b.toString(16).padStart(2, '0')).join('');
      try {
        const response = await fetch(`${this.base()}/v1/blobs/${blob}`, { method: 'PUT', body: Uint8Array.from(sealed),
          headers: { 'content-type': 'application/octet-stream', 'x-zoen-device': this.session.account.device, 'x-zoen-ts': String(ts), 'x-zoen-sig': sig } });
        if (response.ok) return;
        if (response.status === 413) throw Object.assign(new Error('Este arquivo é grande demais.'), { final: true });
        if (response.status < 500 && response.status !== 429) throw Object.assign(new Error('O servidor recusou o arquivo.'), { final: true });
        throw new Error('O servidor não conseguiu receber o arquivo.');
      } catch (error) { if ((error as { final?: boolean }).final || attempt >= 3) throw error instanceof Error && (error as { final?: boolean }).final ? error : new Error('Sem conexão para enviar o arquivo. Tente de novo.'); }
      await new Promise((r) => setTimeout(r, 800 * 2 ** attempt));
    }
  }

  private base(): string { return new URL(this.session.account.relay).origin; }

  /** Fetches, checks and opens a file's chunks. Throws plain Portuguese on failure. */
  async openFile(input: FileDoc, thumbnail = false): Promise<Blob> {
    const file = boundedFile(input);
    const expected = thumbnail ? file.thumb : file;
    if (!expected) throw new Error('Este arquivo não tem prévia.');
    const chunks = thumbnail ? [file.thumb!] : file.chunks;
    const plain = new Uint8Array(expected.bytes);
    let offset = 0;
    for (const chunk of chunks) {
      let sealed: Uint8Array;
      try {
        const response = await fetch(`${this.base()}/v1/blobs/${chunk.blob}`);
        if (!response.ok || !response.body) { await response.body?.cancel(); throw new Error(String(response.status)); }
        const reader = response.body.getReader();
        sealed = new Uint8Array(chunk.bytes + 40); // XChaCha20 nonce + authentication tag.
        let received = 0;
        try {
          for (;;) {
            const { done, value } = await reader.read();
            if (done) break;
            if (value.length > sealed.length - received) throw new Error('Invalid chunk length.');
            sealed.set(value, received); received += value.length;
          }
          if (received !== sealed.length) throw new Error('Invalid chunk length.');
        } finally { await reader.cancel().catch(() => {}); }
      } catch {
        sealed = await this.core.bytes({ kind: 'local_chunk', blob: chunk.blob });
        if (!sealed.length) throw new Error('O arquivo ainda está chegando. Tente de novo em instantes.');
      }
      try {
        const opened = await this.core.bytes({ kind: 'open_chunk', chunk: JSON.stringify(chunk), sealed });
        if (opened.length !== chunk.bytes) throw new Error('Invalid chunk length.');
        plain.set(opened, offset); offset += opened.length;
      }
      catch { throw new Error('Não deu para abrir este arquivo.'); }
    }
    if (offset !== expected.bytes || hex(new Uint8Array(await crypto.subtle.digest('SHA-256', plain))) !== expected.sha256) {
      throw new Error('Não deu para abrir este arquivo.');
    }
    return new Blob([plain], { type: thumbnail ? 'image/png' : file.mime });
  }

  markRead(space: string): Promise<void> {
    if (this.closed || this.connection.kind !== 'ready') return Promise.resolve();
    const conversation = this.conversations.find((c) => c.id === space);
    if (!conversation?.unread) return Promise.resolve();
    const generation = this.generation;
    return this.enqueue(async () => {
      let update: string;
      try { update = await this.core.text({ kind: 'mark_read', space }); }
      catch { throw Object.assign(new Error('Não deu para marcar a conversa como lida. Tente de novo.'), { soft: true }); }
      await this.apply(update, generation);
    }, () => generation !== this.generation || this.connection.kind !== 'ready' ? 'A conexão mudou. Tente de novo.' : null, false);
  }

  private act(space: string, operation: Parameters<MessageCore['text']>[0]): Promise<void> {
    if (this.closed) return Promise.reject(new Error('Recarregue o Zoen para abrir esta sessão de novo.'));
    if (this.connection.kind !== 'ready') return Promise.reject(new Error('Sem conexão agora. Tente de novo em instantes.'));
    if (!this.conversations.find((conversation) => conversation.id === space)?.can_send) {
      return Promise.reject(new Error('Esta conversa ainda está sendo preparada no seu celular.'));
    }
    const generation = this.generation;
    return this.enqueue(async () => {
      this.notice = null;
      let update: string;
      try { update = await this.core.text(operation); }
      catch (error) { throw Object.assign(new Error(plain(message(error))), { soft: true }); }
      await this.apply(update, generation);
    }, () => generation !== this.generation || this.connection.kind !== 'ready' ? 'A conexão mudou. Tente de novo.' : null, false);
  }

  async devices(): Promise<unknown> {
    if (this.closed) throw new Error('Esta sessão está fechada.');
    return this.transport.request('devices');
  }

  async unlinkDevice(device: string): Promise<void> {
    if (this.closed) throw new Error('Esta sessão está fechada.');
    if (device === this.session.account.device) throw new Error('Use Sair para fechar este navegador.');
    await this.transport.request('unlink', device);
  }

  async preview(code: string): Promise<Invite> {
    if (this.closed) throw new Error('The browser session is closed.');
    return parseInvite(await this.transport.request('preview_invite', code));
  }

  join({ invite, code }: { invite: Invite; code: string }): Promise<void> {
    if (this.closed) return Promise.reject(new Error('The browser session is closed.'));
    if (this.connection.kind !== 'ready') return Promise.reject(new Error('The relay is reconnecting. Try again when connected.'));
    const generation = this.generation;
    return this.enqueue(async () => {
      this.notice = null;
      const update = await this.core.text({ kind: 'queue_join', space: invite.space, role: invite.role, code, now: Date.now() });
      await this.apply(update, generation);
    }, () => generation !== this.generation || this.connection.kind !== 'ready' ? 'The connection changed. Try again when connected.' : null);
  }

  private enqueue(operation: () => Promise<void>, refusal: () => string | null = () => null, fatal = true): Promise<void> {
    const task = this.serial.then(async () => {
      if (this.closed) throw new Error('The browser session is closed. Reload Zoen to reopen it.');
      const reason = refusal();
      if (reason) throw new Error(reason);
      try { await operation(); }
      catch (error) { if (fatal || !(error as { soft?: boolean }).soft) this.fail(message(error)); throw error; }
    });
    this.serial = task.catch(() => {});
    return task;
  }

  private fail(reason: string): void {
    if (this.closed) return;
    this.stop();
    this.core.close();
    this.changed({ kind: 'failed', message: reason, conversations: this.conversations });
  }

  stop(): void {
    this.closed = true;
    ++this.generation;
    this.transport.stop();
    this.fileCache.clear();
    this.pendingFiles.clear();
    this.pendingSpaceCommands.clear();
  }
}
