import { array, boolean, bytes, hex, integer, message, object, publicKey, string } from './boundary.ts';
import { persistBeforeOutgoing } from './persistence.ts';
import type { StoredSession } from './session.ts';
import { AuthenticatedTransport, type ConnectionState, type RelayFrame } from './transport.ts';
import type { BrowserVault } from './vault.ts';
import type { CoreWorker } from './worker-client.ts';

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
    thread: value.thread ? string(value.thread) : null, also_in_chat: value.also_in_chat === true,
    thread_following: value.thread_following === true, thread_unread: integer(value.thread_unread ?? 0), thread_mentions: integer(value.thread_mentions ?? 0), replies: integer(value.replies ?? 0),
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
    reply: reply ? { to: string(reply.to), thread: boolean(reply.thread ?? false),
      ...(reply.also_in_chat === true ? { also_in_chat: true as const } : {}) } : null };
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
  return { id: string(value.id), title: string(value.title), kind, can_send: boolean(value.can_send),
    waiting: nullableText(value.waiting), unreadable: integer(value.unreadable),
    unread: integer(value.unread ?? 0), threads_unread: integer(value.threads_unread ?? 0), mentions: integer(value.mentions ?? 0), members: integer(value.members ?? 0), messages: array(value.messages).map(parseMessage) };
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

  constructor({ core, vault, session, changed }: {
    core: MessageCore; vault: MessageVault; session: LinkedSession; changed: (state: MessagesState) => void;
  }) {
    this.core = core; this.vault = vault; this.session = session; this.changed = changed;
    this.transport = new AuthenticatedTransport(core, vault, session.account, (connection) => {
      if (this.closed) return;
      this.connection = connection;
      if (connection.kind !== 'ready') ++this.generation;
      if (connection.kind === 'failed') { this.fail(connection.message); return; }
      this.publish();
    }, (frame, raw) => this.received(frame, raw));
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
    });
  }

  private async apply(json: string, generation: number): Promise<void> {
    const update = parseUpdate(json);
    await persistBeforeOutgoing({ snapshot: { ...this.session, snapshot: update.snapshot },
      save: (snapshot) => this.vault.save(snapshot), frames: update.frames, emit: (frame) => {
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
  post({ space, text, replyTo = '', thread = false, forwarded = false, alsoInChat = false }: { space: string; text: string; replyTo?: string; thread?: boolean; forwarded?: boolean; alsoInChat?: boolean }): Promise<void> {
    return this.act(space, { kind: 'queue_post', space, text, replyTo, thread, forwarded, alsoInChat, now: Date.now() });
  }

  /** Reaction, edit or delete for everyone, in the shared annotation shape. */
  annotate(space: string, target: string, note: { React: { emoji: string } } | 'Unreact' | { Edit: { text: string } } | 'Delete'): Promise<void> {
    return this.act(space, { kind: 'queue_annotation', space, target, note: JSON.stringify(note), now: Date.now() });
  }

  setThreadFollow(space: string, root: string, follow: boolean | null): Promise<void> {
    return this.act(space, { kind: 'set_thread_follow', space, root, follow, now: Date.now() });
  }

  markThreadRead(space: string, root: string): Promise<void> {
    return this.act(space, { kind: 'mark_thread_read', space, root, now: Date.now() });
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
  }
}
