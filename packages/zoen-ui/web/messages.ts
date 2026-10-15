import { array, boolean, bytes, integer, message, object, publicKey, string } from './boundary.ts';
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
  return { id: string(value.id), entry: value.entry ? string(value.entry) : null, author: publicKey(value.author), name: string(value.name), text: string(value.text),
    at_ms: timestamp(value.at_ms), delivery, failure: nullableText(value.failure), ticks: ticks as 'sent' | 'delivered' | 'read' | null,
    edited: value.edited === true, deleted: value.deleted === true, forwarded: value.forwarded === true,
    reactions: array(value.reactions ?? []).map((r) => { const x = object(r); return { emoji: string(x.emoji), count: integer(x.count), mine: x.mine === true }; }),
    quote: quote ? { id: string(quote.id), name: string(quote.name), text: string(quote.text) } : null,
    thread: value.thread ? string(value.thread) : null, replies: integer(value.replies ?? 0),
    repliers: array(value.repliers ?? []).map(string), last_reply_ms: typeof value.last_reply_ms === 'number' ? value.last_reply_ms : null,
    file: value.file ? parseFile(value.file) : null };
}

const HASH = /^[0-9a-f]{64}$/;
function parseChunk(input: unknown) {
  const v = object(input);
  const chunk = { sha256: string(v.sha256), bytes: integer(v.bytes), blob: string(v.blob), key: string(v.key) };
  if (![chunk.sha256, chunk.blob, chunk.key].every((h) => HASH.test(h))) throw new Error('Invalid file chunk.');
  return chunk;
}
function parseFile(input: unknown) {
  const v = object(input);
  return { name: string(v.name), mime: string(v.mime), bytes: integer(v.bytes), sha256: string(v.sha256),
    chunks: array(v.chunks).map(parseChunk), thumb: v.thumb ? parseChunk(v.thumb) : null };
}
export type FileDoc = ReturnType<typeof parseFile>;
export type Chunk = ReturnType<typeof parseChunk>;

function parseConversation(input: unknown) {
  const value = object(input);
  const kind = string(value.kind);
  if (kind !== 'direct' && kind !== 'group' && kind !== 'community' && kind !== 'personal') throw new Error('Invalid conversation kind.');
  return { id: string(value.id), title: string(value.title), kind, can_send: boolean(value.can_send),
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
  if (/Only the person/i.test(reason)) return 'Só quem enviou pode mudar esta mensagem.';
  if (/32 KB/i.test(reason)) return 'A mensagem está longa demais.';
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
  post({ space, text, replyTo = '', thread = false, forwarded = false }: { space: string; text: string; replyTo?: string; thread?: boolean; forwarded?: boolean }): Promise<void> {
    return this.act(space, { kind: 'queue_post', space, text, replyTo, thread, forwarded, now: Date.now() });
  }

  /** Reaction, edit or delete for everyone, in the shared annotation shape. */
  annotate(space: string, target: string, note: { React: { emoji: string } } | 'Unreact' | { Edit: { text: string } } | 'Delete'): Promise<void> {
    return this.act(space, { kind: 'queue_annotation', space, target, note: JSON.stringify(note), now: Date.now() });
  }

  /** Seal like native, upload every encrypted chunk, then post the file into the chat. */
  async sendFile(space: string, file: { name: string; mime: string; bytes: Uint8Array; thumb?: Uint8Array | null }, progress?: (done: number, total: number) => void): Promise<void> {
    if (file.bytes.length > 25 * 1024 * 1024 * 20) throw new Error('Este arquivo é grande demais para o navegador.');
    let framed: Uint8Array;
    try { framed = await this.core.bytes({ kind: 'seal_file', name: file.name.replace(/[\\/\0]/g, '_').slice(0, 255) || 'arquivo', mime: file.mime || 'application/octet-stream', bytes: file.bytes, thumb: file.thumb ?? new Uint8Array() }); }
    catch { throw new Error('Não deu para preparar este arquivo.'); }
    const length = new DataView(framed.buffer, framed.byteOffset).getUint32(0, true);
    const head = object(JSON.parse(new TextDecoder().decode(framed.subarray(4, 4 + length))));
    const uploads = array(head.uploads).map((u) => { const x = object(u); return { blob: string(x.blob), len: integer(x.len) }; });
    let offset = 4 + length;
    for (const [i, upload] of uploads.entries()) {
      await this.upload(upload.blob, framed.subarray(offset, offset + upload.len));
      offset += upload.len;
      progress?.(i + 1, uploads.length);
    }
    await this.act(space, { kind: 'queue_file', space, doc: JSON.stringify(head.doc), now: Date.now() });
  }

  /** The same encrypted chunks, named again in another chat: nothing to upload. */
  forwardFile(space: string, doc: FileDoc): Promise<void> {
    return this.act(space, { kind: 'queue_file', space, doc: JSON.stringify(doc), now: Date.now() });
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
      } catch (error) { if ((error as { final?: boolean }).final || attempt >= 3) throw error instanceof Error && (error as { final?: boolean }).final ? error : new Error('Sem conexão para enviar o arquivo. Tente de novo.'); }
      await new Promise((r) => setTimeout(r, 800 * 2 ** attempt));
    }
  }

  private base(): string { return new URL(this.session.account.relay).origin; }

  /** Fetches, checks and opens a file's chunks. Throws plain Portuguese on failure. */
  async openFile(chunks: Chunk[], mime: string): Promise<Blob> {
    const parts: BlobPart[] = [];
    for (const chunk of chunks) {
      let sealed: Uint8Array;
      try {
        const response = await fetch(`${this.base()}/v1/blobs/${chunk.blob}`);
        if (!response.ok) throw new Error(String(response.status));
        sealed = new Uint8Array(await response.arrayBuffer());
      } catch { throw new Error('O arquivo ainda está chegando. Tente de novo em instantes.'); }
      try { parts.push(Uint8Array.from(await this.core.bytes({ kind: 'open_chunk', chunk: JSON.stringify(chunk), sealed }))); }
      catch { throw new Error('Não deu para abrir este arquivo.'); }
    }
    return new Blob(parts, { type: mime });
  }

  markRead(space: string): Promise<void> {
    if (this.closed || this.connection.kind !== 'ready') return Promise.resolve();
    const conversation = this.conversations.find((c) => c.id === space);
    if (!conversation?.unread) return Promise.resolve();
    const generation = this.generation;
    return this.enqueue(async () => { await this.apply(await this.core.text({ kind: 'mark_read', space }), generation); }, () => null, false).catch(() => {});
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
