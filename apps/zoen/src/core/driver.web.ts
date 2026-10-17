import { BrowserVault } from '../../../../packages/zoen-ui/web/vault';
import { CoreWorker } from '../../../../packages/zoen-ui/web/worker-client';
import {
  Messages,
  parseDevices,
  type MessagesState,
  type Conversation,
  type Message,
  type Profile,
} from '../../../../packages/zoen-ui/web/messages';
import { Pairing } from '../../../../packages/zoen-ui/web/pairing';
import { unavailableCapability } from '../../../../packages/zoen-ui/web/capabilities';
import type { StoredSession } from '../../../../packages/zoen-ui/web/session';
import type { Persona } from './generated/Persona';
import type { SpaceSummary } from './generated/SpaceSummary';
import assets from './generated/web-assets.json';
import { searchBrowser, searchBrowserMessages } from './browser-search';
import { activateBrowserMediaHost } from './browser-media';
import { browserMediaEntry } from './browser-media-entry';
import { createBrowserMediaDriver } from '../media/driver.web';
import { array, integer, object, string } from '../../../../packages/zoen-ui/web/boundary';
import type {
  Command,
  CommandResult,
  CoreDriver,
  CoreEvent,
  Mode,
  PairingStatus,
  Snapshot,
  TimelineEntry,
} from './types';

/** Browser storage and signing remain in the existing authenticated WASM runtime. */
class BrowserDriver implements CoreDriver {
  readonly media = createBrowserMediaDriver();
  private vault: BrowserVault | null = null;
  private worker: CoreWorker | null = null;
  private messages: Messages | null = null;
  private pairing: Pairing | null = null;
  private session: StoredSession = { kind: 'empty' };
  private state: MessagesState = { kind: 'loading' };
  private listeners = new Set<(event: CoreEvent) => void>();
  private release: (() => void) | undefined;
  private locked: Promise<void> | undefined;
  private generation = 0;
  private disposeMedia: (() => void) | null = null;

  subscribe(listener: (event: CoreEvent) => void) {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }
  private changed(event: CoreEvent = { kind: 'change' }) {
    for (const listener of this.listeners) listener(event);
  }

  async open(mode: Mode): Promise<Snapshot> {
    if (mode !== 'account')
      throw new Error(
        'A demonstração é executada pelo motor nativo. Use uma conta vinculada neste navegador.',
      );
    const generation = ++this.generation;
    const unavailable = await unavailableCapability();
    if (unavailable) throw new Error(unavailable);
    await new Promise<void>((resolve, reject) => {
      const held = new Promise<void>((done) => {
        this.release = done;
      });
      this.locked = navigator.locks
        .request('zoen-browser-client', { ifAvailable: true }, async (lock) => {
          if (!lock)
            throw new Error(
              'O Zoen está aberto em outra aba. Feche a outra aba e tente novamente.',
            );
          const vault = await BrowserVault.open();
          let worker: CoreWorker | null = null;
          try {
            const session = await vault.load();
            worker = await CoreWorker.create(vault, assets.workerIntegrity);
            if (generation !== this.generation) {
              worker.close();
              resolve();
              return;
            }
            this.vault = vault;
            this.worker = worker;
            this.session = session;
            if (session.kind === 'linked') await this.startMessages();
            resolve();
            await held;
          } finally {
            worker?.close();
            vault.close();
          }
        })
        .catch(reject);
    });
    return this.request({ op: 'snapshot' });
  }

  private async startMessages() {
    if (this.session.kind !== 'linked' || !this.vault || !this.worker)
      throw new Error('Este navegador ainda não foi vinculado.');
    const generation = this.generation;
    this.messages = new Messages({
      core: this.worker,
      vault: this.vault,
      session: this.session,
      changed: (state) => {
        if (generation !== this.generation) return;
        this.state = state;
        this.changed(
          state.kind === 'failed' ? { kind: 'error', message: state.message } : { kind: 'change' },
        );
      },
      ephemeral: (frame) => {
        if (generation !== this.generation) return;
        this.changed(
          frame.kind === 'presence'
            ? { kind: 'presence', fromId: frame.identity, detail: frame.online ? '1' : '0' }
            : {
                kind: 'ephemeral',
                spaceId: frame.space,
                fromId: frame.from,
                type: frame.event,
                detail: frame.detail,
              },
        );
      },
    });
    await this.messages.start();
    if (generation === this.generation) {
      this.disposeMedia?.();
      this.disposeMedia = activateBrowserMediaHost(this.messages);
    }
  }

  pair(relay: string, changed: (state: PairingStatus) => void) {
    if (!this.vault || !this.worker) throw new Error('Este navegador ainda está abrindo.');
    this.pairing?.stop();
    const generation = this.generation;
    const pairing = new Pairing(this.vault, this.worker, (state) => {
      if (generation !== this.generation) return;
      if (state.kind === 'linked') {
        this.session = { kind: 'linked', account: state.account, snapshot: '' };
        void this.startMessages()
          .then(() => this.changed())
          .catch((error: unknown) =>
            changed({
              kind: 'failed',
              message:
                error instanceof Error ? error.message : 'Não foi possível abrir as conversas.',
            }),
          );
      } else if (state.kind === 'waiting')
        changed({ kind: 'waiting', code: state.pending.code, check: state.pending.check });
      else if (state.kind === 'failed') changed(state);
      else changed({ kind: 'starting' });
    });
    this.pairing = pairing;
    void pairing.start(relay, this.session).catch((error: unknown) =>
      changed({
        kind: 'failed',
        message: error instanceof Error ? error.message : 'Não foi possível vincular.',
      }),
    );
    return () => pairing.stop();
  }

  private persona(identity: string, name: string): Persona {
    const account = this.session.kind === 'linked' ? this.session.account : null;
    const profile = this.profile(identity);
    const owner = profile?.owner ? this.profile(profile.owner) : null;
    const displayName = profile?.name || (profile?.handle ? `@${profile.handle}` : name);
    return {
      id: identity,
      name: displayName,
      handle: profile?.handle ?? (identity === account?.identity ? account.handle : ''),
      initials: displayName
        .split(/\s+/)
        .slice(0, 2)
        .map((part) => Array.from(part)[0] ?? '')
        .join('')
        .toLocaleUpperCase(),
      kind: profile?.kind ?? 'Person',
      tint_hex: profile?.tint_hex ?? '#8E8E93',
      glyph: profile?.glyph ?? null,
      bio: profile?.bio ?? '',
      owner_id: profile?.owner ?? null,
      owner_name: owner?.name ?? null,
      owner_tint_hex: owner?.tint_hex ?? null,
      is_me: identity === account?.identity,
      is_mine: !!account && profile?.owner === account.identity,
    };
  }
  private profile(identity: string): Profile | undefined {
    for (const conversation of this.conversations()) {
      const profile = conversation.profiles.find((person) => person.id === identity);
      if (profile) return profile;
    }
  }
  private conversations(): Conversation[] {
    return this.state.kind === 'loading' ? [] : this.state.conversations;
  }
  private entries(space: string): TimelineEntry[] {
    const conversation = this.conversations().find((item) => item.id === space);
    if (!conversation) throw new Error('Conversa indisponível neste aparelho.');
    return conversation.messages.map((message) => this.entry(message, conversation));
  }
  private entry(message: Message, conversation: Conversation): TimelineEntry {
    const quoted = conversation.messages.find((candidate) => candidate.entry === message.quote?.id);
    const root = conversation.messages.find((candidate) => candidate.entry === message.thread);
    return {
      id: message.id,
      seq: message.seq ?? '0',
      author: this.persona(message.author, message.name),
      at_ms: String(message.at_ms),
      ...browserMediaEntry(message),
      ...(message.card && !message.deleted
        ? { kind: { Message: { text: message.text, card: message.card } } }
        : {}),
      delivery:
        message.delivery === 'sending'
          ? 'Sending'
          : message.delivery === 'failed'
            ? 'Failed'
            : message.ticks === 'read'
              ? 'Read'
              : message.ticks === 'delivered'
                ? 'Delivered'
                : 'Sent',
      reply_to: message.quote
        ? {
            id: quoted?.id ?? message.quote.id,
            author: this.persona(quoted?.author ?? '', message.quote.name),
            text: message.quote.text,
          }
        : null,
      in_thread: root?.id ?? message.thread,
      thread_replies: message.replies,
      edited: message.edited,
      deleted: message.deleted,
      forwarded: message.forwarded,
      reactions: message.reactions.map((reaction) => ({
        emoji: reaction.emoji,
        mine: reaction.mine,
        people: reaction.participants.map((person) => this.persona(person.identity, person.name)),
      })),
    };
  }
  private snapshot(): Snapshot {
    const account = this.session.kind === 'linked' ? this.session.account : null;
    const conversations: SpaceSummary[] = this.conversations().map((conversation) => {
      const last = conversation.messages.at(-1);
      return {
        id: conversation.id,
        title: conversation.title,
        kind:
          conversation.kind === 'group'
            ? 'Group'
            : conversation.kind === 'community'
              ? 'Community'
              : 'Direct',
        privacy: conversation.privacy,
        members: conversation.roster.map((member) =>
          this.persona(member.id, member.id === account?.identity ? account.name : 'Desconhecido'),
        ),
        counterpart:
          conversation.kind === 'direct'
            ? (conversation.roster
                .filter((member) => member.id !== account?.identity)
                .map((member) => this.persona(member.id, 'Desconhecido'))[0] ?? null)
            : null,
        last_preview: last?.deleted ? 'Mensagem apagada' : (last?.text ?? ''),
        last_author: last ? this.persona(last.author, last.name) : null,
        last_at_ms: String(last?.at_ms ?? 0),
        unread: conversation.unread,
        last_delivery: last ? this.entry(last, conversation).delivery : null,
        pinned: conversation.prefs.pinned,
        archived: conversation.prefs.archived,
        muted:
          conversation.prefs.muted_until_ms !== null &&
          BigInt(conversation.prefs.muted_until_ms) > BigInt(Date.now()),
        muted_until_ms: conversation.prefs.muted_until_ms,
        pending_requests: 0,
        event_count: conversation.event_count,
      };
    });
    const connection = this.state.kind === 'active' ? this.state.connection : null;
    return {
      account: account
        ? {
            identity_id: account.identity,
            device_id: account.device,
            name: account.name,
            handle: account.handle,
            relay_url: account.relay,
            registered: true,
            unlocked: true,
            setup_pending: false,
          }
        : null,
      me: account
        ? { ...this.persona(account.identity, account.name), handle: account.handle }
        : null,
      connection: {
        state:
          connection?.kind === 'ready'
            ? 'online'
            : connection?.kind === 'failed' || connection?.kind === 'stopped'
              ? 'offline'
              : 'connecting',
        synced: connection?.kind === 'ready',
        pending: String(
          this.conversations().reduce(
            (total, chat) =>
              total + chat.messages.filter((message) => message.delivery === 'sending').length,
            0,
          ),
        ),
        error: this.state.kind === 'failed' ? this.state.message : null,
      },
      conversations,
      requests: [],
      agents: [],
    };
  }

  async request<C extends Command>(command: C): Promise<CommandResult<C>> {
    let result: unknown = null;
    const messages = this.messages;
    if (command.op === 'snapshot') {
      const snapshot = this.snapshot();
      if (messages) {
        const [agents, requests, profile] = await Promise.all([
          messages.agentRequest({ op: 'agents' }),
          messages.agentRequest({ op: 'requests' }),
          messages.profileRequest({ op: 'profile', identity_id: snapshot.account!.identity_id }),
        ]);
        snapshot.agents = agents as Snapshot['agents'];
        snapshot.requests = requests as Snapshot['requests'];
        const me = profile as { name: string | null; bio: string | null };
        if (me.name && snapshot.account && snapshot.me) {
          snapshot.account.name = me.name;
          snapshot.me.name = me.name;
          snapshot.me.bio = me.bio ?? '';
          snapshot.me.initials = me.name
            .split(/\s+/)
            .slice(0, 2)
            .map((part) => Array.from(part)[0] ?? '')
            .join('')
            .toUpperCase();
        }
      }
      result = snapshot;
    } else if (!messages) throw new Error('Vincule este navegador pelo seu celular primeiro.');
    else
      switch (command.op) {
        case 'timeline': {
          const entries = this.entries(command.space_id).filter(
            (entry) => entry.in_thread === null,
          );
          const end = command.before
            ? entries.findIndex((entry) => entry.id === command.before)
            : entries.length;
          if (end < 0) throw new Error('A conversa mudou. Abra-a novamente.');
          const start = Math.max(0, end - Math.min(200, Math.max(1, command.limit)));
          result = {
            entries: entries.slice(start, end),
            before: entries[start]?.id ?? null,
            has_more: start > 0,
          };
          break;
        }
        case 'send_message':
          await messages.post({
            space: command.space_id,
            text: command.text,
            replyTo: this.target(command.space_id, command.reply_to),
            thread: command.thread,
          });
          result = this.entries(command.space_id).at(-1);
          break;
        case 'mark_read':
          await messages.markRead(command.space_id);
          break;
        case 'profile':
        case 'update_profile':
        case 'block_person':
        case 'unblock_person':
        case 'blocked_people':
          result = await messages.profileRequest(command);
          break;
        case 'receive_history':
          result = await messages.receiveHistory(Number(command.timeout_ms));
          break;
        case 'react':
          await messages.annotate(
            command.space_id,
            this.target(command.space_id, command.message_id),
            command.emoji ? { React: { emoji: command.emoji } } : 'Unreact',
          );
          break;
        case 'edit_message':
          await messages.annotate(
            command.space_id,
            this.target(command.space_id, command.message_id),
            { Edit: { text: command.text } },
          );
          break;
        case 'delete_message':
          await messages.annotate(
            command.space_id,
            this.target(command.space_id, command.message_id),
            'Delete',
          );
          break;
        case 'thread': {
          const root = this.conversations()
            .find((chat) => chat.id === command.space_id)
            ?.messages.find(
              (message) => message.id === command.root_id || message.entry === command.root_id,
            );
          if (!root) throw new Error('A mensagem original não está disponível neste aparelho.');
          result = this.entries(command.space_id).filter(
            (entry) =>
              entry.id === (root?.id ?? command.root_id) ||
              entry.in_thread === (root?.id ?? command.root_id),
          );
          break;
        }
        case 'message':
          result = this.entries(command.space_id).find((entry) => entry.id === command.message_id);
          if (!result) throw new Error('Mensagem indisponível nesta conversa.');
          break;
        case 'search_messages':
          result = searchBrowserMessages(
            this.conversations(),
            command.query,
            command.space_id,
            command.cursor,
            command.limit,
            (id, name) => this.persona(id, name),
          );
          break;
        case 'universal_search':
          result = searchBrowser(
            this.conversations(),
            command.query,
            command.kinds,
            command.limit,
            (id, name) => this.persona(id, name),
          );
          break;
        case 'forward_message': {
          const source = this.entries(command.source_space).find(
            (entry) => entry.id === command.message_id,
          );
          if (!source || source.deleted || !('Message' in source.kind))
            throw new Error('Esta mensagem não pode ser encaminhada como texto.');
          await messages.post({
            space: command.target_space,
            text: source.kind.Message.text,
            forwarded: true,
          });
          break;
        }
        case 'devices':
          result = parseDevices(
            await messages.devices(),
            this.session.kind === 'linked' ? this.session.account.device : '',
          );
          break;
        case 'unlink_device':
          await messages.unlinkDevice(command.device_id);
          break;
        case 'find_people': {
          const query = command.query.trim().replace(/^@/, '').toLocaleLowerCase();
          if (!query) {
            result = [];
            break;
          }
          const response = object(await messages.rpc('lookup', query));
          if (response.kind !== 'profiles') throw new Error('O servidor não retornou os perfis.');
          result = (
            (await messages.directoryProfiles(array(response.profiles))) as Persona[]
          ).filter((person) => !person.is_me);
          break;
        }
        case 'agent_status': {
          const response = object(await messages.rpc('agent_status', command.agent_id));
          const status = string(response.status);
          if (response.kind !== 'agent_state' || !['active', 'paused', 'revoked'].includes(status))
            throw new Error('O servidor não retornou o estado do agente.');
          result = status === 'active' ? 'Active' : status === 'paused' ? 'Paused' : 'Revoked';
          break;
        }
        case 'control_agent':
        case 'revoke_agent_invite': {
          const response = object(
            await messages.rpc(
              command.op === 'control_agent' ? 'control_agent' : 'revoke_agent_invite',
              command.op === 'control_agent'
                ? JSON.stringify({ agent: command.agent_id, action: command.action.toLowerCase() })
                : command.agent_id,
            ),
          );
          if (response.kind !== 'done') throw new Error('O servidor não confirmou esta ação.');
          break;
        }
        case 'create_invite':
        case 'create_agent_invite': {
          const agent = command.op === 'create_agent_invite';
          const response = object(
            await messages.rpc(
              'create_invite',
              JSON.stringify({
                space: command.op === 'create_invite' ? command.space_id : '',
                agent: command.op === 'create_agent_invite' ? command.agent_id : null,
                role: 'Member',
                max_uses: agent ? 10_000 : 50,
                ttl_secs: (agent ? 365 : 7) * 24 * 3600,
              }),
            ),
          );
          if (response.kind !== 'invite') throw new Error('O servidor não criou o convite.');
          const code = string(response.code);
          const expires = string(response.expires_at_ms);
          if (!/^[0-9]+$/.test(expires) || BigInt(expires) > 9223372036854775807n)
            throw new Error('O convite tem uma validade inválida.');
          result = {
            code,
            link: agent ? `https://tryzoen.com/a/${code}` : `zoen://join/${code}`,
            expires_at_ms: expires,
          };
          break;
        }
        case 'preview_invite': {
          const response = object(await messages.rpc('preview_invite', command.code));
          if (response.kind !== 'preview') throw new Error('Este link não está mais disponível.');
          const records = [response.agent, response.inviter].filter((profile) => profile !== null);
          const people = (await messages.directoryProfiles(records)) as Persona[];
          const persona = (record: unknown) =>
            record ? (people.find((person) => person.id === object(record).id) ?? null) : null;
          result = {
            space_id: string(response.space),
            title: string(response.title),
            members: integer(response.members),
            kind: string(response.invite_kind),
            agent: persona(response.agent),
            inviter: persona(response.inviter),
          };
          break;
        }
        case 'new_mutation_id':
        case 'page_parse_markdown':
        case 'page_block_markdown':
        case 'items':
        case 'item':
        case 'item_at':
        case 'page':
        case 'page_at':
        case 'plan_page':
        case 'page_create':
        case 'page_import_markdown':
        case 'page_apply':
        case 'page_commit':
        case 'page_markdown':
        case 'toggle_plan_line':
        case 'edit_plan_line':
        case 'add_plan_line':
        case 'remove_plan_line':
        case 'restore_plan_version':
        case 'restore_version':
        case 'undo':
          result = await messages.documentRequest(command);
          break;
        case 'agents':
        case 'agent_profile':
        case 'requests':
        case 'standing_decisions':
        case 'set_trust':
        case 'raise_budget':
        case 'decide_request':
        case 'revoke_standing':
        case 'preview_decisions':
        case 'agent_activity':
        case 'app_device_grants':
        case 'revoke_app_device':
          result = await messages.agentRequest(command);
          break;
        case 'mentions': {
          const references = (await messages.agentRequest(command)) as {
            space_id: string;
            space_title: string;
            message_id: string;
          }[];
          result = references.flatMap((reference) => {
            const message = this.conversations()
              .find((chat) => chat.id === reference.space_id)
              ?.messages.find(
                (message) =>
                  message.id === reference.message_id || message.entry === reference.message_id,
              );
            const entry = message
              ? this.entries(reference.space_id).find((entry) => entry.id === message.id)
              : undefined;
            return entry
              ? [{ space_id: reference.space_id, space_title: reference.space_title, entry }]
              : [];
          });
          break;
        }
        case 'app_specs':
        case 'app_resource':
        case 'install_app':
        case 'app_call_tool':
          result = await messages.appRequest(command);
          break;
        case 'set_typing':
          await messages.typing(command.space_id, command.typing);
          break;
        default:
          throw new Error('Este recurso ainda está sendo integrado ao cliente web.');
      }
    return result as CommandResult<C>;
  }
  private target(space: string, id: string | null): string {
    return (
      this.conversations()
        .find((chat) => chat.id === space)
        ?.messages.find((message) => message.id === id)?.entry ??
      id ??
      ''
    );
  }
  async createAccount(): Promise<Snapshot> {
    throw new Error('Crie sua conta no celular e vincule este navegador.');
  }
  async close() {
    ++this.generation;
    this.disposeMedia?.();
    this.disposeMedia = null;
    this.pairing?.stop();
    this.messages?.stop();
    this.worker?.close();
    this.release?.();
    this.pairing = null;
    this.messages = null;
    this.worker = null;
    this.vault = null;
    this.session = { kind: 'empty' };
    this.state = { kind: 'loading' };
    const locked = this.locked;
    this.locked = undefined;
    this.release = undefined;
    await locked;
  }
}
export function createDriver(): CoreDriver {
  return new BrowserDriver();
}
