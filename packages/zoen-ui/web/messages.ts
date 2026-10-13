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
  return { id: string(value.id), author: publicKey(value.author), name: string(value.name), text: string(value.text),
    at_ms: timestamp(value.at_ms), delivery, failure: nullableText(value.failure) };
}

function parseConversation(input: unknown) {
  const value = object(input);
  const kind = string(value.kind);
  if (kind !== 'direct' && kind !== 'group' && kind !== 'community' && kind !== 'personal') throw new Error('Invalid conversation kind.');
  const removed = boolean(value.removed);
  const canSend = boolean(value.can_send);
  if (removed && canSend) throw new Error('A removed conversation cannot send.');
  const conversation = { id: string(value.id), title: string(value.title), kind,
    waiting: nullableText(value.waiting), unreadable: integer(value.unreadable), messages: array(value.messages).map(parseMessage) };
  return removed ? { ...conversation, removed: true as const, can_send: false as const }
    : { ...conversation, removed: false as const, can_send: canSend };
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
  const retry = value.retry_at_ms;
  return { snapshot: string(value.snapshot), frames: array(value.frames).map(bytes),
    conversations: array(value.conversations).map(parseConversation),
    retry_at_ms: retry === null ? null : integer(retry) };
}

export function retryAtMs(json: string): number | null {
  return parseUpdate(json).retry_at_ms;
}

export type RetryTimer = { clear(): void };

const MAX_TIMER_MS = 2 ** 31 - 1;

function timerDelay(ms: number): number {
  if (!Number.isFinite(ms) || ms <= 0) return 0;
  return ms > MAX_TIMER_MS ? MAX_TIMER_MS : ms;
}

export function watchRetry(
  current: RetryTimer | null,
  retryAt: number | null,
  now: () => number,
  schedule: (delayMs: number, fire: () => void) => RetryTimer,
  advance: (now: number) => void,
): RetryTimer | null {
  current?.clear();
  if (retryAt === null) return null;
  let active: RetryTimer | null = null;
  const step = () => {
    const remaining = retryAt - now();
    if (remaining <= 0) {
      advance(now());
      return;
    }
    active = schedule(timerDelay(remaining), step);
  };
  step();
  return { clear() { active?.clear(); } };
}

const OUT_OF_DATE = 'This browser session is out of date. Pair this browser again.';
const DUPLICATE_INVITE = 'This invitation is already pending.';

const CORE_REFUSAL_PT: Readonly<Record<string, string>> = {
  'Invalid invitation.': 'Este convite não é válido.',
  'Write a message of up to 32 KB.': 'Escreva uma mensagem de até 32 KB.',
  'This conversation is still active.': 'Esta conversa ainda está ativa.',
  'You are no longer a member of this conversation.': 'Você não faz mais parte desta conversa.',
  'You can read this conversation.': 'Você pode ler esta conversa.',
  'Catching up with the conversation…': 'Sincronizando esta conversa…',
  'Conversation not found.': 'Não encontrei esta conversa.',
  'Open this conversation in the native app.': 'Abra esta conversa no aplicativo.',
  'Waiting for your phone to add this browser to the encrypted conversation…': 'Esperando o celular incluir este navegador na conversa criptografada…',
  'Waiting for the encrypted membership update…': 'Esperando a atualização de quem está na conversa…',
  'Waiting to catch up with the encrypted conversation…': 'Esperando alcançar a conversa criptografada…',
  'The encrypted conversation has conflicting membership. Open it on your phone.': 'A conversa criptografada tem membros em conflito. Abra no celular.',
};

function inviteCopy(language: string): string {
  if (language.toLowerCase().startsWith('pt')) return 'Este convite já está a caminho.';
  return 'This invitation is already on its way.';
}

function refusalCopy(language: string, reason: string): string {
  if (reason === DUPLICATE_INVITE) return inviteCopy(language);
  if (!language.toLowerCase().startsWith('pt')) return reason;
  return CORE_REFUSAL_PT[reason] ?? reason;
}

type CoreEffect =
  | { kind: 'unpaired' }
  | { kind: 'refused'; message: string }
  | { kind: 'fatal'; message: string };

function coreEffect(reason: string, scope: 'action' | 'session', language: string): CoreEffect {
  if (reason === OUT_OF_DATE) return { kind: 'unpaired' };
  if (reason === DUPLICATE_INVITE || scope === 'action') return { kind: 'refused', message: refusalCopy(language, reason) };
  return { kind: 'fatal', message: reason };
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
  private readonly unpaired: () => void;
  private readonly language: string;
  private retry: RetryTimer | null = null;

  constructor({ core, vault, session, changed, unpaired, language }: {
    core: MessageCore; vault: MessageVault; session: LinkedSession; changed: (state: MessagesState) => void;
    unpaired: () => void; language?: string;
  }) {
    this.core = core; this.vault = vault; this.session = session; this.changed = changed; this.unpaired = unpaired;
    this.language = language ?? (typeof navigator === 'undefined' ? 'en' : navigator.language);
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
    this.arm(update.retry_at_ms);
    this.publish();
  }

  private arm(retryAt: number | null): void {
    this.retry = watchRetry(this.retry, retryAt, () => Date.now(), (delay, fire) => {
      const id = setTimeout(fire, timerDelay(delay));
      return { clear() { clearTimeout(id); } };
    }, () => {
      void this.enqueue(async () => {
        if (this.closed || this.connection.kind !== 'ready') return;
        const generation = this.generation;
        const update = await this.core.text({ kind: 'advance', now: Date.now() });
        await this.apply(update, generation);
      });
    });
  }

  deleteConversation(space: string): Promise<void> {
    if (this.closed) return Promise.reject(new Error('The browser session is closed.'));
    const generation = this.generation;
    return this.enqueue(async () => {
      const update = await this.core.text({ kind: 'delete_conversation', space });
      await this.apply(update, generation);
    }, () => null, 'action');
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
    }, () => generation !== this.generation || this.connection.kind !== 'ready' ? 'The connection changed. Try again when connected.' : null, 'action');
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
    }, () => generation !== this.generation || this.connection.kind !== 'ready' ? 'The connection changed. Try again when connected.' : null, 'action');
  }

  private enqueue(operation: () => Promise<void>, refusal: () => string | null = () => null, scope: 'action' | 'session' = 'session'): Promise<void> {
    const task = this.serial.then(async () => {
      if (this.closed) throw new Error('The browser session is closed. Reload Zoen to reopen it.');
      const blocked = refusal();
      if (blocked) throw new Error(blocked);
      try { await operation(); }
      catch (error) {
        const effect = coreEffect(message(error), scope, this.language);
        switch (effect.kind) {
          case 'unpaired':
            try { await this.vault.save({ kind: 'empty' }); }
            catch (saveError) { this.fail(message(saveError)); throw saveError; }
            // Pairing calls create_link on this same worker. Closing it leaves the next attempt dead.
            this.stop();
            this.unpaired();
            return;
          case 'refused':
            throw new Error(effect.message);
          case 'fatal':
            this.fail(effect.message);
            throw error;
          default: {
            const exhaustive: never = effect;
            return exhaustive;
          }
        }
      }
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
    this.retry?.clear();
    this.retry = null;
    this.transport.stop();
  }
}
