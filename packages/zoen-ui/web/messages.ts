import { array, boolean, bytes, hex, integer, message, object, publicKey, string } from './boundary.ts';
import { persistBeforeOutgoing } from './persistence.ts';
import {historySlice,type HistoryResult} from './history.ts';
import type { StoredSession } from './session.ts';
import { AuthenticatedTransport, type ConnectionState, type RelayFrame } from './transport.ts';
import type { BrowserVault } from './vault.ts';
import type { CoreWorker } from './worker-client.ts';

function nullableText(value: unknown): string | null { return value === null ? null : string(value); }
function timestamp(value: unknown): number {
  if (typeof value !== 'number' || !Number.isSafeInteger(value)) throw new Error('Invalid message timestamp.');
  return value;
}
function parseItemEdit(input: unknown, space: string) {
  if (input === null || input === undefined) return null;
  const value = object(input);
  const space_id = string(value.space_id);
  const item_id = string(value.item_id);
  const version = integer(value.version);
  if (space_id !== space || !item_id || version < 2 || version > 4294967295)
    throw new Error('Invalid mini-app activity identity or version.');
  return { space_id, item_id, title: string(value.title), version,
    note: string(value.note), is_undo: boolean(value.is_undo) };
}
function exactKeys(value: Record<string, unknown>, expected: readonly string[]) {
  if (Object.keys(value).sort().join(',') !== [...expected].sort().join(',')) throw new Error('Invalid attachment metadata.');
}
export type MessageAttachmentMedia = Readonly<{
  presentation: 'media' | 'file';
  group: {id: string; index: number; count: number} | null;
}>;
function parseMessageMedia(input: unknown): MessageAttachmentMedia | null {
  if (input === undefined || input === null) return null;
  const value = object(input);
  exactKeys(value, ['presentation', ...(Object.hasOwn(value, 'group') ? ['group'] : [])]);
  if (value.presentation !== 'media' && value.presentation !== 'file') throw new Error('Invalid attachment presentation.');
  let group: {id: string; index: number; count: number} | null = null;
  if (value.group !== undefined && value.group !== null) {
    const raw = object(value.group);
    exactKeys(raw, ['id', 'index', 'count']);
    const id = string(raw.id), index = integer(raw.index), count = integer(raw.count);
    if (!/^album_[a-f0-9]{64}$/.test(id) || count < 2 || count > 10 || index >= count) throw new Error('Invalid attachment group.');
    group = {id, index, count};
  }
  return {presentation: value.presentation, group};
}
function parseMessage(input: unknown, space: string) {
  const value = object(input);
  const delivery = string(value.delivery);
  if (delivery !== 'confirmed' && delivery !== 'sending' && delivery !== 'failed') throw new Error('Invalid message delivery state.');
  const ticks = value.ticks === null || value.ticks === undefined ? null : string(value.ticks);
  if (ticks !== null && ticks !== 'sent' && ticks !== 'delivered' && ticks !== 'read') throw new Error('Invalid message receipt.');
  const quote = value.quote ? object(value.quote) : null;
  const sequence = value.seq === null || value.seq === undefined ? null : string(value.seq);
  if (sequence !== null && (!/^[0-9]+$/.test(sequence) || BigInt(sequence) > 18446744073709551615n)) throw new Error('Invalid message sequence.');
  let text = string(value.text);
  const item_edit = parseItemEdit(value.item_edit, space);
  if (item_edit && (text !== item_edit.note || value.card || value.file || value.quote || value.thread ||
    value.deleted === true || value.edited === true || value.forwarded === true || value.media ||
    array(value.reactions ?? []).length))
    throw new Error('Invalid mixed mini-app activity.');
  let file: FileDoc | null = null;
  let fileItemId: string | null = null;
  let fileVersion: number | null = null;
  let media: ReturnType<typeof parseMessageMedia> = null;
  // A bad attachment must not prevent saving or restoring the conversation's sync state.
  if (value.file) {
    try {
      file = parseFile(value.file);
      fileItemId = string(value.file_item_id);
      fileVersion = integer(value.file_version);
      if (!fileItemId || fileVersion < 1) throw new Error('Invalid file identity.');
      media = parseMessageMedia(value.media);
      if (media && (file.voice || (value.forwarded === true && media.group))) throw new Error('Invalid attachment context.');
    }
    catch { file = null; fileItemId = null; fileVersion = null; media = null; text = 'Anexo indisponível'; }
  }
  return { id: string(value.id), entry: value.entry ? string(value.entry) : null, seq: sequence, author: publicKey(value.author), name: string(value.name), text,
    at_ms: timestamp(value.at_ms), delivery, failure: nullableText(value.failure), ticks: ticks as 'sent' | 'delivered' | 'read' | null,
    edited: value.edited === true, deleted: value.deleted === true, forwarded: value.forwarded === true,
    reactions: array(value.reactions ?? []).map((r) => { const x = object(r); return {
      emoji: string(x.emoji), count: integer(x.count), mine: x.mine === true,
      participants: array(x.participants ?? []).map((p) => { const participant = object(p); return { identity: publicKey(participant.identity), name: string(participant.name) }; }),
    }; }),
    quote: quote ? { id: string(quote.id), name: string(quote.name), text: string(quote.text) } : null,
    thread: value.thread ? string(value.thread) : null, replies: integer(value.replies ?? 0),
    repliers: array(value.repliers ?? []).map(string), last_reply_ms: typeof value.last_reply_ms === 'number' ? value.last_reply_ms : null,
    file, media: value.deleted === true ? null : media, file_item_id: fileItemId, file_version: fileVersion, card: value.card ? parseCard(value.card) : null, item_edit };
}

function parseCard(input: unknown) {
  const value = object(input);
  const app = value.app === null ? null : object(value.app);
  const trust = app ? string(app.trust) : null;
  if (trust && !['Listen', 'Suggest', 'Act', 'Autonomous'].includes(trust)) throw new Error('Invalid mini-app trust.');
  return {item_id: string(value.item_id), title: string(value.title), summary: string(value.summary),
    kind_label: string(value.kind_label), kind_id: string(value.kind_id), version: integer(value.version),
    total_cents: value.total_cents === null ? null : decimal(value.total_cents, 9223372036854775807n, true),
    budget_cents: value.budget_cents === null ? null : decimal(value.budget_cents, 9223372036854775807n, true),
    line_count: integer(value.line_count), done_count: integer(value.done_count),
    app: app ? {app_id: string(app.app_id), resource_uri: string(app.resource_uri), name: string(app.name),
      view_json: string(app.view_json), headline: string(app.headline),
      metrics: array(app.metrics).map((metric) => {const parsed = object(metric); if (typeof parsed.value !== 'number' || !Number.isFinite(parsed.value)) throw new Error('Invalid mini-app metric.'); return {label: string(parsed.label), value: parsed.value};}),
      last_action: nullableText(app.last_action), trust: trust as 'Listen' | 'Suggest' | 'Act' | 'Autonomous', snapshot_json: string(app.snapshot_json)} : null};
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
export type MessageMedia = NonNullable<ReturnType<typeof parseMessageMedia>>;

// Keep received media within the same bounds as client-core/src/files.rs.
const MAX_FILE_BYTES = 512 << 20;
const MAX_CHUNK_BYTES = 16 << 20;
const MAX_THUMBNAIL_BYTES = 2 << 20;
function boundedFile(input: FileDoc, maxChunk = MAX_CHUNK_BYTES): FileDoc {
  const invalid = () => new Error('Arquivo inválido ou grande demais para abrir.');
  if (!input.chunks.length || input.chunks.length > MAX_FILE_BYTES / (1 << 20) + 1) throw invalid();
  const file = parseFile(input); // Snapshot the manifest before any asynchronous work.
  if (!file.bytes || file.bytes > MAX_FILE_BYTES || !HASH.test(file.sha256)) throw invalid();
  if (file.chunks.some((c) => !c.bytes || c.bytes > maxChunk) ||
      file.chunks.reduce((total, c) => total + c.bytes, 0) !== file.bytes ||
      (file.thumb && (!file.thumb.bytes || file.thumb.bytes > MAX_THUMBNAIL_BYTES))) throw invalid();
  const metadata = new TextEncoder().encode(file.name + file.mime + file.sha256).length +
    (file.chunks.length + Number(!!file.thumb)) * (3 * 64 + 8);
  if (metadata > 128 << 10) throw invalid();
  return file;
}

function decimal(value: unknown, max: bigint, signed = false): string {
  const text = string(value);
  if (!(signed ? /^-?[0-9]+$/ : /^[0-9]+$/).test(text) || BigInt(text) > max || BigInt(text) < (signed ? -max - 1n : 0n)) throw new Error('Invalid exact integer.');
  return text;
}
function parseProfile(input: unknown) {
  const value = object(input);
  const kind = string(value.kind);
  if (kind !== 'Person' && kind !== 'Agent') throw new Error('Invalid identity kind.');
  return { id: publicKey(value.id), kind: kind as 'Person' | 'Agent', name: string(value.name), handle: string(value.handle),
    tint_hex: string(value.tint_hex), glyph: nullableText(value.glyph), bio: string(value.bio),
    owner: value.owner === null ? null : publicKey(value.owner),
    photo_sha256: value.photo_sha256 === undefined || value.photo_sha256 === null ? null : publicKey(value.photo_sha256),
    photo_ready: value.photo_ready === undefined ? false : boolean(value.photo_ready),
    photo_mime: value.photo_mime === undefined || value.photo_mime === null ? null : string(value.photo_mime) };
}
export type Profile = ReturnType<typeof parseProfile>;

export function parseConversation(input: unknown) {
  const value = object(input);
  const space = string(value.id);
  const kind = string(value.kind);
  if (kind !== 'direct' && kind !== 'group' && kind !== 'community' && kind !== 'personal') throw new Error('Invalid conversation kind.');
  const privacy = string(value.privacy);
  if (privacy !== 'EndToEnd' && privacy !== 'Closed' && privacy !== 'Public') throw new Error('Invalid conversation privacy.');
  const roster = Object.entries(object(value.roster)).map(([id, input]) => {
    const role = string(input);
    if (role !== 'Owner' && role !== 'Admin' && role !== 'Member' && role !== 'Reader') throw new Error('Invalid member role.');
    return { id: publicKey(id), role };
  });
  const prefs = object(value.prefs);
  return { id: string(value.id), title: string(value.title), kind, privacy: privacy as 'EndToEnd' | 'Closed' | 'Public', roster, profiles: array(value.profiles).map(parseProfile),
    prefs: { pinned: boolean(prefs.pinned), archived: boolean(prefs.archived), muted_until_ms: prefs.muted_until_ms === null ? null : decimal(prefs.muted_until_ms, 9223372036854775807n, true) },
    event_count: decimal(value.event_count, 18446744073709551615n), can_send: boolean(value.can_send),
    waiting: nullableText(value.waiting), integrity_error: nullableText(value.integrity_error ?? null), unreadable: integer(value.unreadable),
    unread: integer(value.unread ?? 0), mentions: integer(value.mentions ?? 0), members: integer(value.members ?? 0), messages: array(value.messages).map((message) => parseMessage(message, space)) };
}
export type Conversation = ReturnType<typeof parseConversation>;
export type Message = ReturnType<typeof parseMessage>;

export function parseDevices(input: unknown, current: string) {
  const response = object(input);
  if (response.kind !== 'devices') throw new Error('O servidor não retornou os dispositivos.');
  return array(response.devices).map((input) => {
    const device = object(input);
    const device_id = publicKey(device.device);
    return { device_id, revoked: boolean(device.revoked), this_device: device_id === current };
  });
}

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
    conversations: array(value.conversations).map(parseConversation),
    maintenance: value.maintenance_ms==null?null:integer(value.maintenance_ms),
    historyReady:value.history_ready===true };
}

/** Core refusals arrive in English; people see plain Portuguese. */
function plain(reason: string): string {
  if (/no longer available/i.test(reason)) return 'Essa mensagem não está mais disponível.';
  if (/foi apagada/i.test(reason)) return 'Essa mensagem foi apagada.';
  if (/Only the person/i.test(reason)) return 'Só quem enviou pode mudar esta mensagem.';
  if (/32 KB/i.test(reason)) return 'A mensagem não pode ficar vazia nem ser longa demais.';
  return 'Não deu para fazer isso agora. Tente de novo.';
}

/** Exact user selection intent; keep this object unchanged for a failed Send retry. */
export type AttachmentSendOptions = Readonly<{
  presentation: 'media' | 'file';
  grouping: 'separate' | 'grouped';
}>;
function attachmentOptions(input: unknown, count: number): AttachmentSendOptions | undefined {
  if (count < 1 || count > 10) throw new Error('Selecione de um a dez arquivos.');
  if (input === undefined) return undefined;
  const value = object(input);
  exactKeys(value, ['presentation', 'grouping']);
  if ((value.presentation !== 'media' && value.presentation !== 'file') ||
      (value.grouping !== 'separate' && value.grouping !== 'grouped') ||
      (value.grouping === 'grouped' && count < 2)) throw new Error('Opções de envio inválidas.');
  if (value.presentation === 'media' && value.grouping === 'separate') return undefined;
  return Object.freeze({presentation: value.presentation, grouping: value.grouping});
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
  private handleChanges:Promise<void>=Promise.resolve();
  private closed = false;
  private attachmentProducer: string | null = null;
  private connection: ConnectionState = { kind: 'connecting' };
  private generation = 0;
  private conversations: Conversation[] = [];
  private notice: string | null = null;
  private readonly aborted = new AbortController();
  private historyTimer: ReturnType<typeof setTimeout> | undefined;
  private maintenanceTimer: ReturnType<typeof setTimeout> | undefined;
  private maintenanceDue=0;
  private historyRunning = false;
  private historyResult: HistoryResult | null = null;
  private historyError = 'O histórico aguarda o celular conectar.';
  private readonly historyWaiters = new Set<{resolve:(result:HistoryResult)=>void;reject:(error:Error)=>void;timer:ReturnType<typeof setTimeout>}>();
  private readonly ephemeral: ((frame: Extract<RelayFrame,{kind:'ephemeral'|'presence'}>) => void) | undefined;

  constructor({ core, vault, session, changed, ephemeral }: {
    core: MessageCore; vault: MessageVault; session: LinkedSession; changed: (state: MessagesState) => void;
    ephemeral?: (frame: Extract<RelayFrame,{kind:'ephemeral'|'presence'}>) => void;
  }) {
    this.ephemeral=ephemeral;
    this.core = core; this.vault = vault; this.session = session; this.changed = changed;
    this.transport = new AuthenticatedTransport(core, vault, session.account, (connection) => {
      if (this.closed) return;
      this.connection = connection;
      if (connection.kind !== 'ready') { ++this.generation; clearTimeout(this.maintenanceTimer); }
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
    if(frame.kind==='presence'||frame.kind==='ephemeral') {
      this.ephemeral?.(frame);
      if(frame.kind==='presence'||frame.event!=='read'&&frame.event!=='delivered')return Promise.resolve();
    }
    const generation = this.generation;
    return this.enqueue(async () => {
      if (generation !== this.generation || this.connection.kind !== 'ready') return;
      if(frame.kind==='ready'||frame.kind==='pong')await this.core.text({kind:'claim_clock',value:this.transport.claimClock});
      const update = frame.kind === 'ready' ? await this.core.text({ kind: 'bootstrap' }) :
        await this.core.text({ kind: 'receive', bytes: raw, now: Date.now() });
      if (frame.kind === 'rejected' && frame.permanent) this.notice = frame.reason;
      await this.apply(update, generation);
      {
        try { await this.resumeDocumentUploads(generation); }
        catch (error) {
          if (!(error as {documentUploadRetry?: boolean}).documentUploadRetry) throw error;
          this.notice = 'Uma página aguarda conexão para terminar de enviar. O rascunho foi preservado.';
          this.scheduleMaintenance(5000,generation,true);this.publish();
        }
        await this.resumeOutgoingHistory(generation);
        if(frame.kind==='ready')this.scheduleHistory();
      }
      if (frame.kind === 'sync_done') this.scheduleHistory();
    });
  }

  private async apply(json: string, generation: number): Promise<void> {
    const update = parseUpdate(json);
    await persistBeforeOutgoing({ snapshot: { ...this.session, snapshot: update.snapshot },
      save: (snapshot) => this.vault.save(snapshot), frames: update.frames, emit: (frame) => {
        // A reconnect bootstraps again from the saved outbox; never emit stale frames.
        if (this.closed || this.connection.kind !== 'ready' || generation !== this.generation) return;
        this.transport.sendRetained(frame);
      } });
    if (this.closed) return;
    this.conversations = update.conversations;
    this.scheduleMaintenance(update.maintenance, generation);
    if(update.historyReady)this.scheduleHistory();
    this.publish();
  }

  private scheduleMaintenance(delay:number|null,generation:number,onlyEarlier=false):void {
    const due=delay===null?0:Date.now()+delay;
    if(onlyEarlier&&this.maintenanceTimer&&this.maintenanceDue<=due)return;
    clearTimeout(this.maintenanceTimer);
    this.maintenanceDue=due;
    if(delay===null||this.closed||this.connection.kind!=='ready'||generation!==this.generation)return;
    if(delay<1||delay>60_000)throw new Error('Intervalo inválido do núcleo.');
    this.maintenanceTimer=setTimeout(()=>{
      this.maintenanceTimer=undefined;
      this.maintenanceDue=0;
      void this.enqueue(async()=>{
        if(this.closed||generation!==this.generation||this.connection.kind!=='ready')return;
        await this.apply(await this.core.text({kind:'maintenance',now:Date.now()}),generation);
        await this.resumeOutgoingHistory(generation);
        try{await this.resumeDocumentUploads(generation);}catch(error){
          if(!(error as {documentUploadRetry?:boolean}).documentUploadRetry)throw error;
          this.notice='A conversa aguarda conexão para terminar de enviar.';
          this.scheduleMaintenance(5000,generation,true);this.publish();
        }
      }).catch(()=>{});
    },delay);
  }

  membershipRequest(command:unknown):Promise<unknown>{return this.domainRequest(command,'membership_request');}

  send({ space, text }: { space: string; text: string }): Promise<void> {
    return this.act(space, { kind: 'queue_message', space, text, now: Date.now() }, true);
  }

  /** Post with an optional inline reply, thread reply or forward. Refusals stay non-fatal. */
  post({ space, text, replyTo = '', thread = false, forwarded = false }: { space: string; text: string; replyTo?: string; thread?: boolean; forwarded?: boolean }): Promise<void> {
    return this.act(space, { kind: 'queue_post', space, text, replyTo, thread, forwarded, now: Date.now() }, true);
  }

  /** Reaction, edit or delete for everyone, in the shared annotation shape. */
  annotate(space: string, target: string, note: { React: { emoji: string } } | 'Unreact' | { Edit: { text: string } } | 'Delete'): Promise<void> {
    return this.act(space, { kind: 'queue_annotation', space, target, note: JSON.stringify(note), now: Date.now() });
  }

  /** Seal like native, upload every encrypted chunk, then post the file into the chat. */
  async sendFile(space: string, file: { name: string; mime: string; bytes: Uint8Array; thumb?: Uint8Array | null; voice?: VoiceMeta; attachment?: AttachmentIntent }, progress?: (done: number, total: number) => void): Promise<{itemId: string; messageId: string}> {
    const options = file.attachment ? attachmentOptions(file.attachment.options, 1) : undefined;
    const attachment = file.attachment ? Object.freeze({mutationId: file.attachment.mutationId,
      caption: file.attachment.caption, replyTo: file.attachment.replyTo, thread: file.attachment.thread,
      ...(options ? {options} : {})}) : undefined;
    if (attachment && file.voice) throw new Error('Este arquivo tem dois tipos de envio. Selecione-o novamente.');
    if (file.bytes.length > MAX_FILE_BYTES) throw new Error('Este arquivo é grande demais para o navegador.');
    let framed: Uint8Array;
    try { framed = await this.core.bytes({ kind: 'seal_file', name: file.name.replace(/[\\/\0]/g, '_').slice(0, 255) || 'arquivo', mime: file.mime || 'application/octet-stream', bytes: file.bytes, thumb: file.thumb ?? new Uint8Array() }); }
    catch { throw new Error('Não deu para preparar este arquivo.'); }
    if (attachment) return this.sendAttachment(space, attachment, framed);
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
      return await this.queueFileRecord(space, doc);
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

  /** All files and their exact ordered intent enter the existing outbox together. */
  async sendFiles(space: string, files: readonly AttachmentBatchFile[], intent: AttachmentBatchIntent): Promise<readonly {itemId: string; messageId: string}[]> {
    const options = attachmentOptions(intent.options, files.length);
    let plainBytes = 0;
    const selected = files.map(file => {
      if (!(file.bytes instanceof Uint8Array) || !file.bytes.length ||
          file.bytes.length > (64 << 20) - plainBytes) throw new Error('O envio precisa ter até 64 MB.');
      plainBytes += file.bytes.length;
      if (file.thumb && (!(file.thumb instanceof Uint8Array) || file.thumb.length > MAX_THUMBNAIL_BYTES)) throw new Error('Prévia inválida.');
      // Caller edits during asynchronous sealing cannot change the admitted intent or bytes.
      return {name: file.name.replace(/[\\/\0]/g, '_').slice(0, 255) || 'arquivo',
        mime: file.mime || 'application/octet-stream', bytes: Uint8Array.from(file.bytes),
        thumb: file.thumb ? Uint8Array.from(file.thumb) : new Uint8Array(),
        mutationId: file.mutationId, caption: file.caption};
    });
    if (new Set(selected.map(file => file.mutationId)).size !== selected.length) throw new Error('Seleção de arquivos inválida.');
    const request = JSON.stringify({space_id: space, reply_to: intent.replyTo, thread: intent.thread,
      ...(options ? {options} : {}), items: selected.map(file => ({mutation_id: file.mutationId, caption: file.caption}))});
    const frames: Uint8Array[] = [];
    let length = 0;
    for (const file of selected) {
      let frame: Uint8Array;
      try { frame = await this.core.bytes({kind: 'seal_file', name: file.name, mime: file.mime, bytes: file.bytes, thumb: file.thumb}); }
      catch { throw new Error('Não deu para preparar estes arquivos.'); }
      if (frame.length < 4 || frame.length > (64 << 20) + MAX_THUMBNAIL_BYTES + (256 << 10)) throw new Error('Arquivo preparado inválido.');
      length += 4 + frame.length;
      if (length > (64 << 20) + 10 * (MAX_THUMBNAIL_BYTES + (256 << 10) + 4)) throw new Error('O envio é grande demais.');
      frames.push(frame);
    }
    const framed = new Uint8Array(length);
    const view = new DataView(framed.buffer);
    let offset = 0;
    for (const frame of frames) {
      view.setUint32(offset, frame.length, true); offset += 4;
      framed.set(frame, offset); offset += frame.length;
    }
    return this.sendAttachmentRequest('queue_attachment_batch', request, framed, value => {
      const posted = array(value);
      if (posted.length !== selected.length) throw new Error('Resposta de envio incompleta.');
      return Object.freeze(posted.map(input => {
        const entry = object(input);
        return Object.freeze({itemId: string(entry.itemId), messageId: string(entry.messageId)});
      }));
    });
  }

  async newAttachmentMutationId(): Promise<string> {
    let mutation = '';
    await this.enqueue(async () => {
      if (!this.attachmentProducer) this.attachmentProducer = await this.core.text({kind: 'attachment_producer', request: JSON.stringify({op: 'begin'})});
      mutation = await this.core.text({kind: 'attachment_producer', request: JSON.stringify({op: 'issue', producer_id: this.attachmentProducer})});
    }, undefined, false);
    return mutation;
  }

  async releaseAttachmentMutation(mutationId: string, accepted: boolean): Promise<void> {
    await this.enqueue(async () => {
      try { await this.core.text({kind: 'attachment_producer', request: JSON.stringify({op: accepted ? 'accepted' : 'drop', mutation_id: mutationId})}); }
      catch (error) { throw Object.assign(new Error(message(error)), {soft: true}); }
    }, undefined, false);
  }

  private async sendAttachment(space: string, intent: AttachmentIntent, framed: Uint8Array): Promise<{itemId: string; messageId: string}> {
    // Freeze exact metadata before entering the serial queue or crossing a reload.
    const request = JSON.stringify({space_id: space, mutation_id: intent.mutationId,
      caption: intent.caption, reply_to: intent.replyTo, thread: intent.thread,
      ...(intent.options ? {options: intent.options} : {})});
    return this.sendAttachmentRequest('queue_attachment', request, framed, value => {
      const posted = object(value);
      return {itemId: string(posted.itemId), messageId: string(posted.messageId)};
    });
  }

  private async sendAttachmentRequest<T>(kind: 'queue_attachment' | 'queue_attachment_batch', request: string, framed: Uint8Array, parse: (value: unknown) => T): Promise<T> {
    let result: T | undefined;
    await this.enqueue(async () => {
      const generation = this.generation;
      let response: Record<string, unknown>;
      try { response = object(JSON.parse(await this.core.text({kind, request, framed, now: Date.now()}))); }
      catch (error) { throw Object.assign(new Error(plain(message(error))), {soft: true}); }
      result = parse(response.result);
      // The existing encrypted snapshot owns the signed events, ciphertext and receipt.
      // No PUT or publish may precede this durable save.
      await this.apply(JSON.stringify(response.update), generation);
      if (!this.closed && generation === this.generation && this.connection.kind === 'ready') {
        try { await this.resumeDocumentUploads(generation); }
        catch (error) {
          if (!(error as {documentUploadRetry?: boolean}).documentUploadRetry) throw error;
          if (this.closed || generation !== this.generation) return;
          this.notice = 'Seu arquivo foi salvo e aguarda conexão para terminar de enviar.';
          this.scheduleMaintenance(5000, generation, true); this.publish();
        }
      }
    }, () => this.closed ? 'Esta sessão está fechada.' : null, false);
    if (!result) throw new Error('Não foi possível guardar o arquivo para enviar.');
    return result;
  }

  private async queueFileRecord(space: string, doc: FileDoc): Promise<{itemId: string; messageId: string}> {
    if (this.closed) throw new Error('Esta sessão está fechada.');
    if (this.connection.kind !== 'ready') throw new Error('Sem conexão agora. Tente de novo em instantes.');
    let result: {itemId: string; messageId: string} | undefined;
    const generation = this.generation;
    await this.enqueue(async () => {
      let response: Record<string, unknown>;
      try { response = object(JSON.parse(await this.core.text({ kind: 'queue_file_record', space, doc: JSON.stringify(doc), now: Date.now() }))); }
      catch (error) { throw Object.assign(new Error(plain(message(error))), {soft: true}); }
      const posted = object(response.result);
      result = {itemId: string(posted.itemId), messageId: string(posted.messageId)};
      await this.apply(JSON.stringify(response.update), generation);
    }, () => generation !== this.generation || this.connection.kind !== 'ready' ? 'A conexão mudou. Tente de novo.' : null, false);
    if (!result) throw new Error('Não foi possível enviar o arquivo.');
    return result;
  }

  async saveProfile(request: {name:string;bio:string;photo:'keep'|'remove'|'set';mime?:string}, bytes:Uint8Array):Promise<void> {
    if(bytes.length>5*1024*1024)throw new Error('A foto precisa ter até 5 MB.');
    const generation=this.generation;
    await this.enqueue(async()=>{
      let response:Record<string,unknown>;
      try {response=object(JSON.parse(await this.core.text({kind:'profile_save',request:JSON.stringify(request),bytes,now:Date.now()})));}
      catch(error){throw Object.assign(new Error(plain(message(error))),{soft:true});}
      await this.apply(JSON.stringify(response.update),generation);
      if(this.connection.kind==='ready') {
        try {await this.resumeDocumentUploads(generation);}
        catch(error) {
          if(!(error as {documentUploadRetry?:boolean}).documentUploadRetry)throw error;
          this.notice='Seu perfil foi salvo e aguarda conexão para enviar a foto.';
          this.scheduleMaintenance(5000,generation,true);this.publish();
        }
      }
    },()=>this.closed||generation!==this.generation?'Esta sessão está fechada.':null,false);
  }

  async profilePhoto(identity:string,expected:string):Promise<{bytes:Uint8Array;mime:string;sha256:string}> {
    if(!/^[a-f0-9]{64}$/.test(identity)||!/^[a-f0-9]{64}$/.test(expected))throw new Error('Foto inválida.');
    let result:{bytes:Uint8Array;mime:string;sha256:string}|undefined;
    const generation=this.generation;
    await this.enqueue(async()=>{
      const read=async()=>{
        const framed=await this.core.bytes({kind:'profile_photo_framed',identity,expected}).catch(error=>{throw Object.assign(new Error(plain(message(error))),{soft:true});});
        if(framed.length<4||framed.length>5*1024*1024+4100)throw new Error('Foto inválida.');
        const length=new DataView(framed.buffer,framed.byteOffset,framed.byteLength).getUint32(0,true);
        if(length>4096||length>framed.length-4)throw new Error('Foto inválida.');
        const header=object(JSON.parse(new TextDecoder().decode(framed.subarray(4,4+length))));
        if(header.identity_id!==identity||header.sha256!==expected||!['image/jpeg','image/png','image/heic','image/webp'].includes(string(header.mime)))throw new Error('Esta foto não corresponde ao perfil.');
        return {header,bytes:framed.slice(4+length)};
      };
      let photo=await read();
      if(!boolean(photo.header.cached)) {
        const blob=string(photo.header.blob);
        if(!/^[a-f0-9]{64}$/.test(blob))throw new Error('Foto inválida.');
        const response=await fetch(`${this.base()}/v1/blobs/${blob}`,{signal:this.aborted.signal});
        if(!response.ok||!response.body){await response.body?.cancel();throw Object.assign(new Error('A foto ainda está chegando. Tente novamente.'),{soft:true});}
        const reader=response.body.getReader();const chunks:Uint8Array[]=[];let size=0;
        try {for(;;){const {done,value}=await reader.read();if(done)break;if(value.length>5*1024*1024+40-size)throw new Error('Foto grande demais.');size+=value.length;chunks.push(value);}}
        finally{await reader.cancel().catch(()=>{});}
        if(this.closed||generation!==this.generation)throw new Error('Esta sessão está fechada.');
        const sealed=new Uint8Array(size);let offset=0;for(const chunk of chunks){sealed.set(chunk,offset);offset+=chunk.length;}
        const update=await this.core.text({kind:'profile_photo_received',identity,expected,sealed});
        await this.apply(update,generation);photo=await read();
      }
      if(!boolean(photo.header.cached)||!photo.bytes.length)throw Object.assign(new Error('A foto ainda não está disponível.'),{soft:true});
      result={bytes:photo.bytes,mime:string(photo.header.mime),sha256:expected};
    },()=>this.closed||generation!==this.generation?'Esta sessão está fechada.':null,false);
    if(!result)throw new Error('A foto ainda não está disponível.');
    return result;
  }

  async fileManifest(itemId: string, version?: number): Promise<{itemId: string; version: number; doc: FileDoc; info: unknown}> {
    const result = object(await this.documentRequest({op: 'file_manifest', item_id: itemId, version: version ?? null}));
    return {itemId: string(result.itemId), version: integer(result.version), doc: parseFile(result.doc), info: result.info};
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
          signal: this.aborted.signal,
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
  async openFile(input: FileDoc, thumbnail = false, pagePayload = false): Promise<Blob> {
    const file = boundedFile(input, pagePayload ? 32 << 20 : MAX_CHUNK_BYTES);
    const expected = thumbnail ? file.thumb : file;
    if (!expected) throw new Error('Este arquivo não tem prévia.');
    const chunks = thumbnail ? [file.thumb!] : file.chunks;
    const plain = new Uint8Array(expected.bytes);
    let offset = 0;
    for (const chunk of chunks) {
      let sealed: Uint8Array;
      try {
        const response = await fetch(`${this.base()}/v1/blobs/${chunk.blob}`, { signal: this.aborted.signal });
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

  private act(space: string, operation: Parameters<MessageCore['text']>[0], allowOffline = false): Promise<void> {
    if (this.closed) return Promise.reject(new Error('Recarregue o Zoen para abrir esta sessão de novo.'));
    if (!allowOffline && this.connection.kind !== 'ready') return Promise.reject(new Error('Sem conexão agora. Tente de novo em instantes.'));
    if (!this.conversations.find((conversation) => conversation.id === space)?.can_send) {
      return Promise.reject(new Error('Esta conversa ainda está sendo preparada no seu celular.'));
    }
    const generation = this.generation;
    return this.enqueue(async () => {
      if (!this.conversations.find((conversation) => conversation.id === space)?.can_send)
        throw Object.assign(new Error('Esta conversa ainda está sendo preparada no seu celular.'), { soft: true });
      this.notice = null;
      let update: string;
      try { update = await this.core.text(operation); }
      catch (error) { throw Object.assign(new Error(plain(message(error))), { soft: true }); }
      await this.apply(update, generation);
    }, () => !allowOffline && (generation !== this.generation || this.connection.kind !== 'ready') ? 'A conexão mudou. Tente de novo.' : null, false);
  }

  /** Shared Page/Plan commands execute inside Rust, then persist before any publication.
   * Downloaded CRDT payloads are verified by the signed descriptor and the Rust cache.
   * A large outgoing page has a durable upload barrier, including across tab reloads.
   */
  async documentRequest(command: unknown): Promise<unknown> {
    return this.domainRequest(command, 'document_request');
  }

  async agentRequest(command: unknown): Promise<unknown> {
    return this.domainRequest(command, 'agent_request');
  }

  async appRequest(command: unknown): Promise<unknown> {
    return this.domainRequest(command, 'app_request');
  }

  async integrityRequest(command: unknown): Promise<unknown> {
    let result: unknown;
    // This is a local, nonpersistent read. Reconnecting the same account does
    // not invalidate its log; stopping this Messages instance does.
    await this.enqueue(async () => {
      try {
        const response = await this.core.text({kind: 'integrity_request', request: JSON.stringify(command)});
        if (this.closed) throw new Error('Esta sessão foi encerrada. Abra o relatório novamente.');
        result = JSON.parse(response);
      }
      catch (error) { throw Object.assign(new Error(message(error)), {soft: true}); }
    }, undefined, false);
    return result;
  }

  async profileRequest(command: unknown): Promise<unknown> {
    return this.domainRequest(command, 'profile_request');
  }

  updateHandle(handle:string):Promise<string>{
    const generation=this.generation;
    const task=this.handleChanges.then(async()=>{
      const active=()=>!this.closed&&this.connection.kind==='ready'&&generation===this.generation;
      if(!active())throw new Error('Conecte ao servidor para mudar seu nome de usuário.');
      const profile=object(JSON.parse(await this.core.text({kind:'handle_profile',handle})));
      const normalized=string(profile.handle);
      let response:Record<string,unknown>;
      try{response=object(await this.rpc('register',JSON.stringify(profile)));}
      catch(error){if(/handle_taken/.test(message(error)))throw new Error('Este nome de usuário já está em uso.');throw error;}
      if(response.kind!=='registered')throw new Error('O servidor não confirmou seu nome de usuário.');
      await this.enqueue(async()=>{
        const update=await this.core.text({kind:'handle_confirmed',profile:JSON.stringify(response.profile),expected:normalized});
        this.session.account.handle=normalized;
        await this.apply(update,generation);
      },()=>active()?null:'A conta ou conexão mudou. Tente novamente.');
      return normalized;
    });
    this.handleChanges=task.then(()=>{},()=>{});return task;
  }

  private scheduleHistory(delay=0):void {
    if(this.closed||this.historyResult||this.connection.kind!=='ready'||this.historyRunning
      ||!this.session.account.link_secret||!this.session.account.hpke_secret)return;
    clearTimeout(this.historyTimer);
    this.historyTimer=setTimeout(()=>{this.historyTimer=undefined;void this.restoreHistory();},delay);
  }

  private async restoreHistory():Promise<void> {
    if(this.historyRunning||this.closed||this.connection.kind!=='ready')return;
    this.historyRunning=true;const generation=this.generation;
    const valid=()=>!this.closed&&generation===this.generation&&this.connection.kind==='ready';
    try {
      const result=await historySlice({base:this.base(),signal:this.aborted.signal,valid,
        rpc:(kind,value)=>this.rpc(kind,value),core:async(operation)=>{
          let work:unknown;
          await this.enqueue(async()=>{
            let response:Record<string,unknown>;
            try{response=object(JSON.parse(await this.core.text(operation)));}
            catch(error){throw Object.assign(new Error(message(error)),{soft:true});}
            await this.apply(JSON.stringify(response.update),generation);work=response.work;
          },()=>valid()?null:'A conexão mudou durante a restauração.',false);
          return work;
        }});
      if(result&&valid()){
        this.historyResult=result;
        for(const waiter of this.historyWaiters){clearTimeout(waiter.timer);waiter.resolve(result);}this.historyWaiters.clear();
      }
    }catch(error){this.historyError=message(error);}
    finally{this.historyRunning=false;this.scheduleHistory(1_000);}
  }

  async historyStatus():Promise<{initial_pending:boolean;manifest_received:boolean;chunks_received:number;chunks_total:number;receive_active:boolean}>{
    let status:ReturnType<Messages['historyStatus']> extends Promise<infer T>?T:never;
    await this.enqueue(async()=>{
      const value=object(JSON.parse(await this.core.text({kind:'history_status'})));
      status={initial_pending:boolean(value.initial_pending),manifest_received:boolean(value.manifest_received),
        chunks_received:integer(value.chunks_received),chunks_total:integer(value.chunks_total),receive_active:this.historyRunning};
    });
    return status!;
  }

  receiveHistory(timeoutMs:number):Promise<HistoryResult>{
    if(this.closed)return Promise.reject(new Error('Esta sessão está fechada.'));
    if(this.historyResult)return Promise.resolve(this.historyResult);
    if(!Number.isSafeInteger(timeoutMs)||timeoutMs<1||timeoutMs>300_000)return Promise.reject(new Error('Tempo de restauração inválido.'));
    this.scheduleHistory();
    return new Promise((resolve,reject)=>{
      const waiter={resolve,reject,timer:setTimeout(()=>{this.historyWaiters.delete(waiter);reject(new Error(this.historyError));},timeoutMs)};
      this.historyWaiters.add(waiter);
    });
  }

  private async domainRequest(command: unknown, domain: 'document_request' | 'agent_request' | 'app_request' | 'profile_request' | 'membership_request' | 'link_request'): Promise<unknown> {
    let result: unknown;
    await this.enqueue(async () => {
      let response: Record<string, unknown>;
      const request = JSON.stringify(command);
      const operation = () => ({kind: domain, request, now: Date.now()});
      try { response = object(JSON.parse(await this.core.text(operation()))); }
      catch (error) { throw Object.assign(new Error(message(error)), { soft: true }); }
      const generation = this.generation;
      if (response.update !== null) await this.apply(JSON.stringify(response.update), generation);
      // The saved snapshot now owns every outgoing byte. An interrupted PUT cannot
      // publish an incomplete document or erase the person's local draft.
      if (array(response.uploads).length && this.connection.kind === 'ready') {
        try { await this.resumeDocumentUploads(generation); }
        catch (error) {
          if (!(error as {documentUploadRetry?: boolean}).documentUploadRetry) throw error;
          this.notice = 'O documento foi salvo neste navegador e aguarda conexão para terminar de enviar.';
          this.scheduleMaintenance(5000,generation,true);this.publish();
        }
      }
      const downloads = array(response.downloads).map(parseChunk);
      if (downloads.length) {
        let complete = true;
        for (const reference of downloads) {
          let bytes: Uint8Array;
          try {
            if (reference.bytes > 32 << 20) throw new Error('Conteúdo grande demais.');
            const blob = await this.openFile({ name: 'page', path: '', mime: 'application/vnd.zoen.page',
              bytes: reference.bytes, sha256: reference.sha256, chunks: [reference], thumb: null, voice: null }, false, true);
            bytes = new Uint8Array(await blob.arrayBuffer());
          } catch { complete = false; continue; }
          const update = await this.core.text({ kind: 'document_cache_blob', reference: JSON.stringify(reference), bytes });
          await this.apply(update, generation);
        }
        if (complete) {
          // Only reads can request missing payloads. Mutations refuse an incomplete
          // page before changing it, so fetching never replays a non-idempotent write.
          try { response = object(JSON.parse(await this.core.text(operation()))); }
          catch (error) { throw Object.assign(new Error(message(error)), { soft: true }); }
          if (response.update !== null) await this.apply(JSON.stringify(response.update), generation);
        } else if (response.result === null) {
          throw Object.assign(new Error('O documento ainda está chegando. Tente de novo quando estiver conectado.'), { soft: true });
        }
      }
      result = response.result;
    }, () => this.closed ? 'Esta sessão está fechada.' : null, false);
    return result;
  }

  private async resumeDocumentUploads(generation: number): Promise<void> {
    const blobs = array(JSON.parse(await this.core.text({ kind: 'document_uploads' }))).map(string);
    for (const blob of blobs) {
      if (this.closed || this.connection.kind !== 'ready' || generation !== this.generation) return;
      const sealed = await this.core.bytes({ kind: 'local_chunk', blob });
      if (!sealed.length) throw new Error('O conteúdo local do documento está indisponível.');
      try { await this.upload(blob, sealed); }
      catch (error) { throw Object.assign(error instanceof Error ? error : new Error('Sem conexão para enviar o documento.'), {documentUploadRetry: true}); }
      const update = await this.core.text({ kind: 'document_uploaded', blobs: JSON.stringify([blob]) });
      await this.apply(update, generation);
    }
  }

  private async linkWork(command:unknown,generation:number):Promise<Record<string,unknown>> {
    let response:Record<string,unknown>;
    try {response=object(JSON.parse(await this.core.text({kind:'link_request',request:JSON.stringify(command),now:Date.now()})));}
    catch(error){throw Object.assign(new Error(message(error)),{soft:true});}
    if(response.update!==null)await this.apply(JSON.stringify(response.update),generation);
    return object(response.result);
  }

  /** Retained ciphertext is saved before PUT, and the same bytes survive a cold retry. */
  private async resumeOutgoingHistory(generation:number):Promise<void> {
    if(this.closed||generation!==this.generation||this.connection.kind!=='ready')return;
    try {
      let work=await this.linkWork({op:'history_send_resume'},generation);
      while(work.kind==='upload') {
        if(this.closed||generation!==this.generation||this.connection.kind!=='ready')return;
        const transfer=publicKey(work.transfer),index=integer(work.index),sha=publicKey(work.sha256);
        const sealed=bytes(work.bytes);
        if(index>899||sealed.length<16||sealed.length>(1<<20)+16)throw new Error('Parte inválida do histórico.');
        const abort=new AbortController();
        const stopped=()=>abort.abort(this.aborted.signal.reason);
        this.aborted.signal.addEventListener('abort',stopped,{once:true});
        const timer=setTimeout(()=>abort.abort(new Error('O envio do histórico demorou demais.')),15_000);
        let response:Response;
        try {
          response=await fetch(`${this.base()}/v1/transfer/${transfer}/${index}`,{method:'PUT',
            headers:{'x-zoen-device':publicKey(work.device),'x-zoen-ts':string(work.timestamp),'x-zoen-sig':string(work.signature)},
            body:sealed,signal:abort.signal}).catch(error=>{throw Object.assign(new Error(message(error)),{historyUploadRetry:true});});
        }finally{clearTimeout(timer);this.aborted.signal.removeEventListener('abort',stopped);}
        await response.body?.cancel();
        if(!response.ok){
          const retry=response.status===429||response.status>=500;
          if(!retry)await this.linkWork({op:'history_failed',transfer,message:'O servidor recusou o envio do histórico. Confira o dispositivo e tente novamente.'},generation);
          throw Object.assign(new Error(retry?'O histórico aguarda conexão para terminar de enviar.':'O servidor recusou o envio do histórico. Confira o dispositivo e tente novamente.'),{historyUploadRetry:retry,soft:true});
        }
        if(this.closed||generation!==this.generation||this.connection.kind!=='ready')return;
        work=await this.linkWork({op:'history_uploaded',transfer,index,sha256:sha},generation);
        // Yield the canonical queue between chunks so sync/receipts stay responsive.
        if(work.kind==='upload'){this.scheduleMaintenance(50,generation,true);return;}
      }
    }catch(error){
      if(this.closed)return;
      if(!(error as {historyUploadRetry?:boolean;soft?:boolean}).historyUploadRetry&&!(error as {soft?:boolean}).soft)throw error;
      this.notice=message(error);
      if((error as {historyUploadRetry?:boolean}).historyUploadRetry)this.scheduleMaintenance(5000,generation,true);
      this.publish();
    }
  }

  linkRequest(command:unknown):Promise<unknown>{return this.domainRequest(command,'link_request');}

  async linkDevice(code:string):Promise<{device_id:string;check:string}> {
    let linked:{device_id:string;check:string}|undefined;
    await this.enqueue(async()=>{
      let response:Record<string,unknown>;
      try{response=object(JSON.parse(await this.core.text({kind:'link_prepare',code,now:Date.now()})));}
      catch(error){throw Object.assign(new Error(message(error)),{soft:true});}
      const result=object(response.result);
      linked={device_id:publicKey(result.device_id),check:string(result.check)};
      await this.apply(JSON.stringify(response.update),this.generation);
    },()=>this.connection.kind==='ready'?null:'Aguarde a conexão para vincular o aparelho.',false);
    const end=Date.now()+30_000;
    while(Date.now()<end&&!this.closed){
      const status=object(await this.linkRequest({op:'link_status',device_id:linked!.device_id}));
      if(status.failure!==null)throw new Error(string(status.failure));
      if(status.delivered===true)return linked!;
      await new Promise((resolve)=>setTimeout(resolve,100));
    }
    throw new Error('O vínculo foi preparado e aguarda confirmação do servidor. Tente novamente com o mesmo código.');
  }

  async linkProgress(device:string):Promise<number[]>{
    return array(await this.linkRequest({op:'link_progress',device_id:device})).map(integer);
  }

  async sendHistory(device:string):Promise<number>{
    await this.linkRequest({op:'history_send',device_id:device});
    await this.enqueue(()=>this.resumeOutgoingHistory(this.generation));
    const end=Date.now()+60_000;
    while(Date.now()<end&&!this.closed){
      const work=object(await this.linkRequest({op:'history_send_resume'}));
      if(work.kind==='complete')return integer(work.messages);
      await new Promise((resolve)=>setTimeout(resolve,200));
    }
    throw new Error('O histórico foi preservado e continuará a ser enviado quando houver conexão.');
  }

  async loadOlder(space:string,timeoutMs:number):Promise<{loaded:number;more:boolean;primary_offline:boolean;message:string|null}>{
    if(!Number.isSafeInteger(timeoutMs)||timeoutMs<1||timeoutMs>300_000)throw new Error('Tempo de carregamento inválido.');
    const request=object(await this.linkRequest({op:'load_older',space_id:space}));
    if(request.request_id===null)return {loaded:0,more:false,primary_offline:false,message:null};
    const id=string(request.request_id),end=Date.now()+timeoutMs;
    while(Date.now()<end&&!this.closed){
      const answer=object(await this.linkRequest({op:'older_status',space_id:space,request_id:id}));
      if(typeof answer.failure==='string')throw new Error(answer.failure);
      if(answer.pending===false)return {loaded:integer(answer.loaded),more:boolean(answer.more),primary_offline:false,message:null};
      await new Promise((resolve)=>setTimeout(resolve,100));
    }
    return {loaded:0,more:true,primary_offline:true,message:'Abra o Zoen no aparelho que vinculou este navegador para carregar mensagens antigas.'};
  }

  async devices(): Promise<unknown> {
    if (this.closed) throw new Error('Esta sessão está fechada.');
    return this.transport.request('devices');
  }

  async rpc(kind:string,value=''):Promise<unknown> {
    if(this.closed)throw new Error('Esta sessão está fechada.');
    const generation=this.generation;
    const response=await this.transport.request(kind,value);
    if(this.closed||generation!==this.generation)throw new Error('A conexão mudou. Tente de novo.');
    return response;
  }

  async directoryProfiles(profiles:unknown[]):Promise<unknown> {
    let result:unknown;
    const generation=this.generation;
    await this.enqueue(async()=>{
      const response=object(JSON.parse(await this.core.text({kind:'directory_profiles',profiles:JSON.stringify(profiles)})));
      await this.apply(JSON.stringify(response.update),generation);result=response.result;
    },()=>generation!==this.generation?'A conexão mudou. Tente de novo.':null,false);
    return result;
  }

  async typing(space:string,typing:boolean):Promise<void> {
    if(this.closed||this.connection.kind!=='ready')return;
    const generation=this.generation;
    const frame=await this.core.bytes({kind:'typing_frame',space,typing});
    if(frame.length&&!this.closed&&this.connection.kind==='ready'&&generation===this.generation)this.transport.send(frame);
  }

  async unlinkDevice(device: string): Promise<void> {
    if (this.closed) throw new Error('Esta sessão está fechada.');
    if (device === this.session.account.device) throw new Error('Use Sair para fechar este navegador.');
    const response=object(await this.rpc('unlink',device));
    if(response.kind!=='done')throw new Error('O servidor não confirmou a remoção deste aparelho.');
    await this.membershipRequest({op:'refresh_devices'});
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
    this.aborted.abort();
    clearTimeout(this.historyTimer);
    clearTimeout(this.maintenanceTimer);
    for(const waiter of this.historyWaiters){clearTimeout(waiter.timer);waiter.reject(new Error('Esta sessão está fechada.'));}this.historyWaiters.clear();
    ++this.generation;
    this.transport.stop();
  }
}
