import type { ClientStore } from '../core/store';
import type { Snapshot } from '../core/generated/Snapshot';
import type { SpaceSummary } from '../core/generated/SpaceSummary';
import { accountScope, accountScopeKey, type AccountScope } from '../core/account-scope.ts';

export type MutedChat = Readonly<Pick<SpaceSummary, 'id' | 'title' | 'muted_until_ms'>>;
export type MutedChatTicket = Readonly<{ scope: string; revision: number }>;
export type MutedChatsState = Readonly<{
  scope: string;
  phase: 'inactive' | 'loading' | 'ready' | 'failed';
  chats: readonly MutedChat[];
  selected: string | null;
  pending: string | null;
  readFailed: boolean;
  actionFailed: boolean;
}>;
export interface MutedChatsPort {
  scope(): AccountScope | null;
  read(): Promise<Snapshot>;
  unmute(id: string, owner: AccountScope): Promise<unknown>;
}
export const emptyMutedChats: MutedChatsState = {
  scope: '',
  phase: 'inactive',
  chats: [],
  selected: null,
  pending: null,
  readFailed: false,
  actionFailed: false,
};
export function createMutedChatsPort(store: ClientStore): MutedChatsPort {
  return {
    scope: () => accountScope(store),
    read: () => store.driver.request({ op: 'snapshot' }),
    unmute: (space_id, owner) => {
      if (accountScopeKey(owner) !== accountScopeKey(accountScope(store)))
        return Promise.reject(new Error('Account scope changed'));
      return store.request({
        op: 'set_chat_muted',
        space_id,
        muted: false,
        expected_identity_id: owner.identity,
        expected_device_id: owner.device,
      });
    },
  };
}
function project(snapshot: Snapshot, owner: AccountScope): readonly MutedChat[] {
  const account = snapshot?.account;
  if (
    !account ||
    !account.registered ||
    !account.unlocked ||
    account.setup_pending ||
    account.identity_id !== owner.identity ||
    account.device_id !== owner.device ||
    account.relay_url !== owner.relay ||
    !Array.isArray(snapshot.conversations)
  )
    throw new Error('Foreign account snapshot');
  const ids = new Set<string>();
  for (const chat of snapshot.conversations) {
    if (
      !chat ||
      typeof chat.id !== 'string' ||
      !chat.id ||
      ids.has(chat.id) ||
      typeof chat.title !== 'string' ||
      typeof chat.muted !== 'boolean' ||
      !Array.isArray(chat.members) ||
      (chat.muted &&
        (typeof chat.muted_until_ms !== 'string' || !/^\d+$/.test(chat.muted_until_ms)))
    )
      throw new Error('Invalid conversation snapshot');
    ids.add(chat.id);
  }
  return snapshot.conversations
    .filter((chat) => chat.muted && chat.members.some((member) => member.id === owner.identity))
    .map(({ id, title, muted_until_ms }) => Object.freeze({ id, title, muted_until_ms }));
}
const sameChat = (left: MutedChat, right: MutedChat) =>
  left.id === right.id &&
  left.title === right.title &&
  left.muted_until_ms === right.muted_until_ms;

/** Disposable focus projection. Core's signed Personal log is the only persistence. */
export class MutedChatsSession {
  private state = emptyMutedChats;
  private owner: AccountScope | null = null;
  private revision = 0;
  // Reads may retire a review, but cannot retire a write already dispatched.
  private readonly inFlight = new Map<string, { id: string }>();
  private readonly listeners = new Set<() => void>();
  private readonly port: MutedChatsPort;
  constructor(port: MutedChatsPort) {
    this.port = port;
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish(patch: Partial<MutedChatsState>) {
    this.state = { ...this.state, ...patch };
    for (const listener of this.listeners) listener();
  }
  activate(owner: AccountScope | null) {
    this.owner = owner ? Object.freeze({ ...owner }) : null;
    ++this.revision;
    const scope = accountScopeKey(owner);
    this.state = { ...emptyMutedChats, scope, pending: this.inFlight.get(scope)?.id ?? null };
    this.publish({});
  }
  retire() {
    this.activate(null);
  }
  private current(revision = this.revision) {
    return Boolean(
      this.owner &&
      revision === this.revision &&
      accountScopeKey(this.owner) === accountScopeKey(this.port.scope()),
    );
  }
  ticket(observed: MutedChatsState = this.state): MutedChatTicket | null {
    return observed === this.state && this.current()
      ? Object.freeze({ scope: this.state.scope, revision: this.revision })
      : null;
  }
  private accepts(ticket: MutedChatTicket | null) {
    return Boolean(ticket && ticket.scope === this.state.scope && this.current(ticket.revision));
  }
  async load(ticket: MutedChatTicket | null, failedAction: string | null = null) {
    if (!this.accepts(ticket) || !this.owner) return;
    const owner = this.owner;
    const revision = ++this.revision;
    this.publish({
      phase: 'loading',
      chats: [],
      selected: null,
      pending: this.inFlight.get(this.state.scope)?.id ?? null,
      readFailed: false,
    });
    try {
      const snapshot = await this.port.read();
      if (!this.current(revision)) return;
      const chats = project(snapshot, owner);
      this.publish({
        phase: 'ready',
        chats,
        actionFailed: Boolean(failedAction && chats.some((chat) => chat.id === failedAction)),
      });
    } catch {
      if (this.current(revision))
        this.publish({ phase: 'failed', readFailed: true, actionFailed: Boolean(failedAction) });
    }
  }
  select(id: string | null, ticket: MutedChatTicket | null) {
    if (
      !this.accepts(ticket) ||
      (id !== null &&
        (this.state.phase !== 'ready' ||
          this.state.pending ||
          !this.state.chats.some((c) => c.id === id)))
    )
      return;
    this.publish({ selected: id });
  }
  canOpen(id: string, ticket: MutedChatTicket | null): boolean {
    return Boolean(
      this.accepts(ticket) &&
      this.state.phase === 'ready' &&
      !this.state.pending &&
      this.state.selected === id &&
      this.state.chats.some((chat) => chat.id === id),
    );
  }
  async unmute(id: string, confirm: () => Promise<boolean>, ticket: MutedChatTicket | null) {
    const chat = this.state.chats.find((entry) => entry.id === id);
    if (
      !this.accepts(ticket) ||
      !this.owner ||
      !chat ||
      this.state.phase !== 'ready' ||
      this.state.pending ||
      this.inFlight.has(this.state.scope) ||
      this.state.selected !== id
    )
      return;
    const revision = this.revision;
    const owner = this.owner;
    const scope = this.state.scope;
    const write = { id };
    let dispatched = false;
    let failed = false;
    this.publish({ pending: id, actionFailed: false });
    try {
      if (!(await confirm()) || !this.current(revision) || this.state.selected !== id) return;
      // Revalidate this exact reviewed chat; never replay stale pin/archive fields.
      const snapshot = await this.port.read();
      if (!this.current(revision) || this.state.selected !== id) return;
      const current = project(snapshot, owner).find((entry) => entry.id === id);
      if (!current || !sameChat(chat, current)) {
        await this.load(this.ticket());
        return;
      }
      this.inFlight.set(scope, write);
      dispatched = true;
      await this.port.unmute(id, owner);
    } catch {
      failed = true;
      if (!dispatched && this.current(revision)) {
        this.publish({ actionFailed: true });
        await this.load(this.ticket(), id);
      }
    } finally {
      if (dispatched && this.inFlight.get(scope) === write) {
        this.inFlight.delete(scope);
        if (this.current() && this.state.scope === scope) {
          // A newer read/foreground may have changed revision during the ACK.
          // Acceptance may precede a lost response; reconcile by read, never resend.
          await this.load(this.ticket(), failed ? id : null);
        }
      } else if (this.current(revision)) this.publish({ pending: null });
    }
  }
}
