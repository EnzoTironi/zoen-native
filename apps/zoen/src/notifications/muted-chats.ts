import type { ClientStore } from '../core/store';
import type { Snapshot } from '../core/generated/Snapshot';
import type { ChatMuteMode } from '../core/generated/ChatMuteMode';
import type { SpaceSummary } from '../core/generated/SpaceSummary';
import { accountScope, accountScopeKey, type AccountScope } from '../core/account-scope.ts';

export type MutedChat = Readonly<Pick<SpaceSummary, 'id' | 'title' | 'muted_until_ms'>>;
export type MuteChoice =
  | Exclude<ChatMuteMode, { kind: 'for' }>
  | { kind: 'for'; duration_seconds: number };
type FailedMute = { id: string; until: string | null | undefined };
export type MutedChatTicket = Readonly<{ scope: string; revision: number }>;
export type MutedChatsState = Readonly<{
  scope: string;
  phase: 'inactive' | 'loading' | 'ready' | 'failed';
  chats: readonly MutedChat[];
  selected: string | null;
  pending: string | null;
  readFailed: boolean;
  actionFailed: boolean;
  clock_ms: string | null;
}>;
export interface MutedChatsPort {
  scope(): AccountScope | null;
  read(): Promise<Snapshot>;
  unmute(id: string, owner: AccountScope): Promise<unknown>;
  mute(
    id: string,
    mode: ChatMuteMode,
    observed: string | null,
    owner: AccountScope,
  ): Promise<unknown>;
}
export const emptyMutedChats: MutedChatsState = {
  scope: '',
  phase: 'inactive',
  chats: [],
  selected: null,
  pending: null,
  readFailed: false,
  actionFailed: false,
  clock_ms: null,
};
export function createMutedChatsPort(store: ClientStore): MutedChatsPort {
  return {
    scope: () => accountScope(store),
    read: () => store.driver.request({ op: 'snapshot' }),
    mute: (space_id, mode, expected_muted_until_ms, owner) => {
      if (accountScopeKey(owner) !== accountScopeKey(accountScope(store)))
        return Promise.reject(new Error('Account scope changed'));
      return store.request({
        op: 'set_chat_mute',
        space_id,
        mode,
        expected_muted_until_ms,
        expected_identity_id: owner.identity,
        expected_device_id: owner.device,
      });
    },
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
function project(snapshot: Snapshot, owner: AccountScope, spaceId?: string): readonly MutedChat[] {
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
    .filter(
      (chat) =>
        (spaceId ? chat.id === spaceId : chat.muted) &&
        chat.members.some((member) => member.id === owner.identity),
    )
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
  private readonly spaceId?: string;
  constructor(port: MutedChatsPort, spaceId?: string) {
    this.port = port;
    this.spaceId = spaceId;
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
  async load(ticket: MutedChatTicket | null, failedAction: FailedMute | null = null) {
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
      const chats = project(snapshot, owner, this.spaceId);
      const actual = failedAction
        ? (chats.find((chat) => chat.id === failedAction.id)?.muted_until_ms ?? null)
        : null;
      // No remaining effect after Core's fresh clock passes the requested window.
      // This retires the intent; it does not claim the lost write was acknowledged.
      const elapsed = Boolean(
        failedAction?.until &&
        failedAction.until !== '9223372036854775807' &&
        snapshot.clock_ms &&
        /^[0-9]+$/.test(snapshot.clock_ms) &&
        /^[0-9]+$/.test(failedAction.until) &&
        BigInt(failedAction.until) <= BigInt(snapshot.clock_ms) &&
        actual === null,
      );
      this.publish({
        phase: 'ready',
        chats,
        clock_ms: snapshot.clock_ms ?? null,
        actionFailed: Boolean(
          failedAction &&
          !elapsed &&
          (failedAction.until === undefined || actual !== failedAction.until),
        ),
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
    return this.change(id, confirm, ticket);
  }
  async mute(id: string, choice: MuteChoice, ticket: MutedChatTicket | null) {
    return this.change(id, async () => true, ticket, choice);
  }
  private async change(
    id: string,
    confirm: () => Promise<boolean>,
    ticket: MutedChatTicket | null,
    choice?: MuteChoice,
  ) {
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
    let target: string | null | undefined = choice ? undefined : null;
    this.publish({ pending: id, actionFailed: false });
    try {
      if (!(await confirm()) || !this.current(revision) || this.state.selected !== id) return;
      // Revalidate this exact reviewed chat; never replay stale pin/archive fields.
      const snapshot = await this.port.read();
      if (!this.current(revision) || this.state.selected !== id) return;
      const current = project(snapshot, owner, this.spaceId).find((entry) => entry.id === id);
      if (!current || !sameChat(chat, current)) {
        await this.load(this.ticket());
        return;
      }
      if (choice) {
        let mode: ChatMuteMode;
        if (choice.kind === 'for') {
          if (!snapshot.clock_ms || !/^[1-9][0-9]*$/.test(snapshot.clock_ms))
            throw new Error('Canonical clock unavailable');
          mode = { ...choice, from_ms: snapshot.clock_ms };
          target = (BigInt(snapshot.clock_ms) + BigInt(choice.duration_seconds) * 1000n).toString();
        } else {
          mode = choice;
          target =
            mode.kind === 'forever'
              ? '9223372036854775807'
              : mode.kind === 'until'
                ? mode.until_ms
                : null;
        }
        this.inFlight.set(scope, write);
        dispatched = true;
        await this.port.mute(id, mode, current.muted_until_ms, owner);
      } else {
        this.inFlight.set(scope, write);
        dispatched = true;
        await this.port.unmute(id, owner);
      }
    } catch {
      failed = true;
      if (!dispatched && this.current(revision)) {
        this.publish({ actionFailed: true });
        await this.load(this.ticket(), { id, until: target });
      }
    } finally {
      if (dispatched && this.inFlight.get(scope) === write) {
        this.inFlight.delete(scope);
        if (this.current() && this.state.scope === scope) {
          // A newer read/foreground may have changed revision during the ACK.
          // Acceptance may precede a lost response; reconcile by read, never resend.
          await this.load(this.ticket(), failed ? { id, until: target } : null);
        }
      } else if (this.current(revision)) this.publish({ pending: null });
    }
  }
}
