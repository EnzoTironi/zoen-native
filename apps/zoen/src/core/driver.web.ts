import { authorityJson } from '../../../../packages/zoen-ui/web/authority';
import { unavailableCapability } from '../../../../packages/zoen-ui/web/capabilities';
import {
  Messages,
  type Conversation,
  type Message,
  type MessagesState,
  type Profile,
} from '../../../../packages/zoen-ui/web/messages';
import { Pairing } from '../../../../packages/zoen-ui/web/pairing';
import { enrollPrimary } from '../../../../packages/zoen-ui/web/primary';
import type { StoredSession } from '../../../../packages/zoen-ui/web/session';
import { BrowserVault } from '../../../../packages/zoen-ui/web/vault';
import { CoreWorker } from '../../../../packages/zoen-ui/web/worker-client';
import { locale as appLocale } from '../i18n/index.ts';
import { createBrowserMediaDriver } from '../media/driver.web';
import { createBrowserProfilePhotoDriver } from '../profile/driver.web';
import { browserSnapshot, browserTimelineEntry } from './browser-driver-projection';
import { activateBrowserMediaHost } from './browser-media';
import { browserMessageCommand } from './browser-message-commands';
import type { Persona } from './generated/Persona';
import type { ProfileDto } from './generated/ProfileDto';
import assets from './generated/web-assets.json';
import type {
  Command,
  CommandResult,
  CoreDriver,
  CoreEvent,
  DeviceScope,
  Mode,
  PairingStatus,
  Snapshot,
  TimelineEntry,
} from './types';

/** Browser storage and signing remain in the existing authenticated WASM runtime. */
class BrowserDriver implements CoreDriver {
  readonly media = createBrowserMediaDriver();
  readonly photos = createBrowserProfilePhotoDriver();
  private authorityReady = false;
  private erasing = false;
  private erasure: Promise<void> | null = null;
  private erasedScope: DeviceScope | null = null;
  canAuthorizeDevice() {
    return this.authorityReady && this.session.kind === 'linked';
  }
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
  private providerAbort: AbortController | null = null;
  private selfProfile: {
    name: string;
    bio: string;
    photo_sha256: string | null;
    photo_ready: boolean;
    photo_mime: string | null;
  } | null = null;

  private authRoute() {
    return window.location.pathname.replace(/\/$/, '') === '/sign-in';
  }

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
    if (this.erasing) throw new Error('O apagamento deste navegador precisa ser concluído.');
    if (mode !== 'account')
      throw new Error(
        'A demonstração é executada pelo motor nativo. Use uma conta vinculada neste navegador.',
      );
    const generation = ++this.generation;
    const unavailable = await unavailableCapability({ onboarding: this.authRoute() });
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
            if ((session.kind === 'linked') === this.authRoute()) {
              this.session = session;
              window.location.replace(session.kind === 'linked' ? '/' : '/sign-in');
              resolve();
              return;
            }
            worker = await CoreWorker.create(vault, assets.workerIntegrity, appLocale);
            if (generation !== this.generation) {
              worker.close();
              resolve();
              return;
            }
            this.vault = vault;
            this.worker = worker;
            this.session = session;
            if (session.kind === 'linked') {
              const authority = await vault.authority();
              if (authority) {
                await worker.text({
                  kind: 'primary_restore',
                  device: await vault.device(),
                  account: JSON.stringify(session.account),
                  authority: authorityJson(authority),
                });
                this.authorityReady = true;
              }
            }
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
    const account = this.session.account;
    this.messages = new Messages({
      core: this.worker,
      vault: this.vault,
      session: this.session,
      changed: (state) => {
        if (generation !== this.generation) return;
        const previousConnection = this.state.kind === 'active' ? this.state.connection.kind : null;
        this.state = state;
        if (
          (state.kind === 'active' && state.connection.kind !== previousConnection) ||
          (state.kind === 'failed' && previousConnection !== null)
        )
          this.changed({ kind: 'connection' });
        this.changed(
          state.kind === 'failed' ? { kind: 'error', message: state.message } : { kind: 'change' },
        );
      },
      ephemeral: (frame) => {
        if (generation !== this.generation) return;
        this.changed(
          frame.kind === 'presence'
            ? { kind: 'presence', identityId: frame.identity, online: frame.online }
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
      this.disposeMedia = activateBrowserMediaHost(this.messages, account.identity, account.device);
    }
  }

  pair(relay: string, changed: (state: PairingStatus) => void, expectedIdentityId?: string) {
    if (this.erasing) throw new Error('Este navegador está sendo apagado.');
    if (!this.vault || !this.worker) throw new Error('Este navegador ainda está abrindo.');
    if (this.session.kind === 'provider_pending')
      throw new Error(
        'Suas chaves desta entrada estão preservadas. Conclua ou recupere essa conta antes de vincular outra.',
      );
    this.pairing?.stop();
    const generation = this.generation;
    const pairing = new Pairing(this.vault, this.worker, (state) => {
      if (generation !== this.generation) return;
      if (state.kind === 'linked') {
        this.session = { kind: 'linked', account: state.account, snapshot: '' };
        if (this.authRoute()) {
          window.location.replace('/');
          return;
        }
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
    void pairing.start(relay, this.session, expectedIdentityId).catch((error: unknown) => {
      if (generation !== this.generation) return;
      changed({
        kind: 'failed',
        message: error instanceof Error ? error.message : 'Não foi possível vincular.',
      });
    });
    return () => pairing.stop();
  }

  private persona(identity: string, name: string): Persona {
    const account = this.session.kind === 'linked' ? this.session.account : null;
    const profile = this.profile(identity);
    const owner = profile?.owner ? this.profile(profile.owner) : null;
    const own = identity === account?.identity ? this.selfProfile : null;
    const displayName =
      own?.name || profile?.name || (profile?.handle ? `@${profile.handle}` : name);
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
      bio: own?.bio ?? profile?.bio ?? '',
      owner_id: profile?.owner ?? null,
      owner_name: owner?.name ?? null,
      owner_tint_hex: owner?.tint_hex ?? null,
      photo_sha256: own?.photo_sha256 ?? profile?.photo_sha256 ?? null,
      photo_ready: own?.photo_ready ?? profile?.photo_ready ?? false,
      photo_mime: own?.photo_mime ?? profile?.photo_mime ?? null,
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
    return browserTimelineEntry(message, conversation, (identity, name) =>
      this.persona(identity, name),
    );
  }
  private snapshot(): Snapshot {
    const account =
      this.session.kind === 'linked' || this.session.kind === 'provider_pending'
        ? this.session.account
        : null;
    return browserSnapshot({
      account: account
        ? {
            identity: account.identity,
            device: account.device,
            name: account.name,
            handle: account.handle,
            relay: account.relay,
          }
        : null,
      registered: this.session.kind === 'linked',
      setupPending: this.session.kind === 'provider_pending',
      ownName: this.selfProfile?.name ?? null,
      connection: this.state.kind === 'active' ? this.state.connection : null,
      error: this.state.kind === 'failed' ? this.state.message : null,
      sourceConversations: this.conversations(),
      persona: (identity, name) => this.persona(identity, name),
      entry: (message, conversation) => this.entry(message, conversation),
    });
  }

  async request<C extends Command>(command: C): Promise<CommandResult<C>> {
    if (this.erasing) throw new Error('Este navegador está sendo apagado.');
    const requestGeneration = this.generation;
    let result: unknown = null;
    const messages = this.messages;
    if (command.op === 'provider_sign_in') {
      const vault = this.vault;
      const core = this.worker;
      if (!vault || !core) throw new Error('Este navegador ainda está abrindo.');
      this.pairing?.stop();
      this.providerAbort?.abort();
      const abort = new AbortController();
      this.providerAbort = abort;
      const generation = this.generation;
      const linked = await enrollPrimary({
        vault,
        core,
        relay: command.relay_url,
        proof: command.proof_token,
        name: command.name,
        signal: abort.signal,
        pending: (pending) => {
          if (generation === this.generation) {
            this.session = pending;
            this.changed();
          }
        },
      });
      if (generation !== this.generation) throw new Error('Esta sessão foi encerrada.');
      this.session = linked;
      result = this.snapshot().account;
      if (this.authRoute()) {
        window.location.replace('/');
      } else {
        await this.startMessages();
        this.changed();
      }
    } else if (command.op === 'snapshot') {
      let snapshot = this.snapshot();
      if (messages) {
        const [agents, requests, profile, warnings] = await Promise.all([
          messages.agentRequest({ op: 'agents' }),
          messages.agentRequest({ op: 'requests' }),
          messages.profileRequest({ op: 'profile', identity_id: snapshot.account!.identity_id }),
          messages.integrityRequest({ op: 'integrity_warnings' }),
        ]);
        if (requestGeneration !== this.generation)
          throw new Error('A conta mudou. Abra esta tela novamente.');
        const me = profile as ProfileDto;
        if (me.name)
          this.selfProfile = {
            name: me.name,
            bio: me.bio ?? '',
            photo_sha256: me.photo_sha256,
            photo_ready: me.photo_ready,
            photo_mime: me.photo_mime,
          };
        snapshot = this.snapshot();
        snapshot.agents = agents as Snapshot['agents'];
        snapshot.requests = requests as Snapshot['requests'];
        snapshot.integrity_warnings = {
          ...snapshot.integrity_warnings,
          ...(warnings as Snapshot['integrity_warnings']),
        };
      }
      result = snapshot;
    } else if (!messages) throw new Error('Vincule este navegador pelo seu celular primeiro.');
    else if (
      command.op === 'profile' ||
      command.op === 'update_profile' ||
      command.op === 'block_person' ||
      command.op === 'unblock_person' ||
      command.op === 'blocked_people'
    ) {
      result = await messages.profileRequest(command);
      if (command.op === 'update_profile' && this.session.kind === 'linked') {
        const own = (await messages.profileRequest({
          op: 'profile',
          identity_id: this.session.account.identity,
        })) as ProfileDto;
        if (requestGeneration !== this.generation)
          throw new Error('A conta mudou. Abra esta tela novamente.');
        if (own.name)
          this.selfProfile = {
            name: own.name,
            bio: own.bio ?? '',
            photo_sha256: own.photo_sha256,
            photo_ready: own.photo_ready,
            photo_mime: own.photo_mime,
          };
        this.changed();
      }
    } else {
      result = (
        await browserMessageCommand(command, messages, {
          entries: (space) => this.entries(space),
          conversations: () => this.conversations(),
          target: (space, id) => this.target(space, id),
          persona: (identity, name) => this.persona(identity, name),
          account: () => this.snapshot().account,
          currentDevice: () => (this.session.kind === 'linked' ? this.session.account.device : ''),
        })
      ).result;
    }
    if (requestGeneration !== this.generation)
      throw new Error('A conta mudou. Abra esta tela novamente.');
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
    throw new Error('Entre com Apple ou Google para continuar.');
  }
  async eraseDevice(expected: DeviceScope): Promise<void> {
    const scope = { ...expected };
    const account =
      this.session.kind === 'linked' || this.session.kind === 'provider_pending'
        ? this.session.account
        : null;
    if (account && (account.identity !== scope.identity_id || account.device !== scope.device_id))
      throw new Error('A conta mudou. Confirme o aparelho novamente.');
    if (!account) {
      if (
        this.erasedScope?.identity_id === scope.identity_id &&
        this.erasedScope.device_id === scope.device_id
      )
        return;
      throw new Error('A conta mudou. Confirme o aparelho novamente.');
    }
    if (this.erasure) return this.erasure;
    const vault = this.vault;
    if (!vault) throw new Error('Este navegador ainda está abrindo.');
    this.erasing = true;
    // Keep the exclusive browser lock until durable storage deletion finishes.
    this.authorityReady = false;
    ++this.generation;
    this.providerAbort?.abort();
    this.providerAbort = null;
    this.disposeMedia?.();
    this.disposeMedia = null;
    this.pairing?.stop();
    this.messages?.stop();
    this.worker?.close();
    this.pairing = null;
    this.messages = null;
    this.worker = null;
    this.state = { kind: 'loading' };
    this.selfProfile = null;
    this.erasure = (async () => {
      await vault.erase();
      this.erasedScope = scope;
      await this.finishClose();
      this.erasing = false;
    })().finally(() => {
      this.erasure = null;
    });
    return this.erasure;
  }

  async close() {
    // Unmounting cannot release the lock while erasure is still pending.
    if (this.erasure) await this.erasure;
    if (this.erasing) throw new Error('Tente apagar este navegador novamente.');
    await this.finishClose();
  }

  private async finishClose() {
    this.authorityReady = false;
    ++this.generation;
    this.providerAbort?.abort();
    this.providerAbort = null;
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
    this.selfProfile = null;
    const locked = this.locked;
    this.locked = undefined;
    this.release = undefined;
    await locked;
  }
}
export function createDriver(): CoreDriver {
  return new BrowserDriver();
}
