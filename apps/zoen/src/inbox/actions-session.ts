import { accountScopeKey, type AccountScope } from '../core/account-scope.ts';
import type { Snapshot } from '../core/generated/Snapshot';
import type { SpaceSummary } from '../core/generated/SpaceSummary';
import type { ChatLayoutInput } from '../core/generated/ChatLayoutInput';
import type { ChatLayoutResult } from '../core/generated/ChatLayoutResult';
import type { InboxAction } from './row-actions';

type Receipt = NonNullable<ChatLayoutResult['receipt']>;
export type InboxUndo = { id: string; title: string; receipt: Receipt };
export type InboxState = {
  scope: string;
  pending: string | null;
  undo: InboxUndo | null;
  error: 'change' | 'undo' | null;
};
export type InboxTicket = { scope: string; generation: number };
export interface InboxActionsPort {
  scope(): AccountScope | null;
  member(id: string): boolean;
  snapshot(): Promise<Snapshot>;
  /** Admitted writes refresh the shared ClientStore before resolving. */
  layout(input: ChatLayoutInput): Promise<ChatLayoutResult>;
  newId(): Promise<string>;
  mute(id: string, muted: boolean, observed: string | null, owner: AccountScope): Promise<unknown>;
  read(id: string): Promise<unknown>;
  refresh(): Promise<unknown>;
}
const empty = (): InboxState => ({ scope: '', pending: null, undo: null, error: null });

/** Disposable action/receipt presentation. Core owns prefs, revisions and Undo admission. */
export class InboxActionsSession {
  private state = empty();
  private generation = 0;
  private owner: AccountScope | null = null;
  private readonly listeners = new Set<() => void>();
  private readonly port: InboxActionsPort;
  constructor(port: InboxActionsPort) {
    this.port = port;
  }
  getSnapshot = () => this.state;
  subscribe = (fn: () => void) => {
    this.listeners.add(fn);
    return () => {
      this.listeners.delete(fn);
    };
  };
  private publish(patch: Partial<InboxState>) {
    this.state = { ...this.state, ...patch };
    for (const fn of this.listeners) fn();
  }
  activate(owner: AccountScope | null) {
    ++this.generation;
    this.owner = owner ? { ...owner } : null;
    this.state = { ...empty(), scope: accountScopeKey(owner) };
    this.publish({});
  }
  retire() {
    this.activate(null);
  }
  private current(generation = this.generation) {
    return Boolean(
      this.owner &&
      generation === this.generation &&
      this.state.scope === accountScopeKey(this.port.scope()),
    );
  }
  ticket(observed = this.state): InboxTicket | null {
    return observed === this.state && this.current()
      ? { scope: this.state.scope, generation: this.generation }
      : null;
  }
  private accepts(ticket: InboxTicket | null) {
    return Boolean(ticket && ticket.scope === this.state.scope && this.current(ticket.generation));
  }
  canOpen(id: string, ticket: InboxTicket | null) {
    return this.accepts(ticket) && !this.state.pending && this.port.member(id);
  }
  dismiss(ticket: InboxTicket | null) {
    if (this.accepts(ticket)) this.publish({ undo: null, error: null });
  }
  private chat(snapshot: Snapshot, id: string, owner: AccountScope) {
    const account = snapshot.account;
    if (
      !account?.registered ||
      !account.unlocked ||
      account.setup_pending ||
      account.identity_id !== owner.identity ||
      account.device_id !== owner.device ||
      account.relay_url !== owner.relay
    )
      throw Error('Account changed');
    const chat = snapshot.conversations.find(
      (item) => item.id === id && item.members.some((m) => m.id === owner.identity),
    );
    if (!chat) throw Error('Conversation unavailable');
    return chat;
  }
  private scope(id: string, owner: AccountScope) {
    return { space_id: id, expected_identity_id: owner.identity, expected_device_id: owner.device };
  }
  private async dispatch(
    input: Extract<ChatLayoutInput, { kind: 'change' | 'undo' }>,
    generation: number,
  ) {
    try {
      return { result: await this.port.layout(input), acknowledged: true };
    } catch (error) {
      if (!this.current(generation)) throw error;
      // Reconcile this exact ID, never generate a retry intent or overwrite a newer revision.
      return {
        result: await this.port.layout({
          kind: 'reconcile',
          scope: input.scope,
          operation_id: input.operation_id,
        }),
        acknowledged: false,
      };
    }
  }
  async run(chat: SpaceSummary, action: InboxAction, ticket: InboxTicket | null) {
    if (!this.accepts(ticket) || this.state.pending || !this.owner || !this.port.member(chat.id))
      return;
    const generation = ++this.generation,
      owner = this.owner;
    this.publish({ pending: chat.id, error: null, undo: null });
    try {
      const fresh = this.chat(await this.port.snapshot(), chat.id, owner);
      if (!this.current(generation)) return;
      if (action === 'pin' || action === 'archive') {
        const field = action === 'pin' ? 'pinned' : 'archived',
          value = !chat[field];
        const scope = this.scope(chat.id, owner);
        const observed = await this.port.layout({ kind: 'read', scope });
        if (!this.current(generation)) return;
        if (observed.current[field] === value) {
          await this.port.refresh();
          return;
        }
        const operation_id = await this.port.newId();
        if (!this.current(generation) || !this.port.member(chat.id)) return;
        const reply = await this.dispatch(
          {
            kind: 'change',
            scope,
            operation_id,
            field,
            value,
            observed_revision:
              field === 'pinned'
                ? observed.current.pin_revision
                : observed.current.archive_revision,
          },
          generation,
        );
        if (!this.current(generation)) return;
        const { result, acknowledged } = reply;
        const receipt = result.receipt;
        if (!receipt && acknowledged && result.current[field] === value) return;
        if (
          !receipt ||
          receipt.operation_id !== operation_id ||
          receipt.field !== field ||
          receipt.value !== value
        )
          throw Error('Unconfirmed operation');
        if (!acknowledged) await this.port.refresh();
        if (this.current(generation) && receipt.undo_available)
          this.publish({ undo: { id: chat.id, title: fresh.title, receipt } });
      } else if (action === 'mute') {
        const value = !chat.muted;
        if (fresh.muted !== value) {
          try {
            await this.port.mute(chat.id, value, fresh.muted_until_ms, owner);
          } catch (error) {
            if (!this.current(generation)) return;
            const latest = this.chat(await this.port.snapshot(), chat.id, owner);
            if (latest.muted_until_ms !== (value ? '9223372036854775807' : null)) throw error;
            if (this.current(generation)) await this.port.refresh();
          }
        } else if (this.current(generation)) await this.port.refresh();
      } else if (action === 'read') {
        if (fresh.unread > 0) await this.port.read(chat.id);
        else if (this.current(generation)) await this.port.refresh();
      }
    } catch {
      if (this.current(generation)) {
        this.publish({ error: 'change' });
        try {
          await this.port.refresh();
        } catch {}
      }
    } finally {
      if (this.current(generation)) this.publish({ pending: null });
    }
  }
  async undo(observed: InboxUndo, ticket: InboxTicket | null) {
    if (
      !this.accepts(ticket) ||
      this.state.pending ||
      this.state.undo !== observed ||
      !this.owner ||
      !this.port.member(observed.id)
    )
      return;
    const generation = ++this.generation,
      owner = this.owner;
    this.publish({ pending: observed.id, error: null });
    try {
      this.chat(await this.port.snapshot(), observed.id, owner);
      if (!this.current(generation)) return;
      const operation_id = await this.port.newId();
      if (!this.current(generation) || !this.port.member(observed.id)) return;
      const { result, acknowledged } = await this.dispatch(
        {
          kind: 'undo',
          scope: this.scope(observed.id, owner),
          operation_id,
          original_operation_id: observed.receipt.operation_id,
          observed_revision: observed.receipt.revision,
        },
        generation,
      );
      if (!this.current(generation)) return;
      if (
        result.receipt?.operation_id !== operation_id ||
        result.receipt.undo_of !== observed.receipt.operation_id
      )
        throw Error('Unconfirmed undo');
      if (!acknowledged) await this.port.refresh();
      if (this.current(generation)) this.publish({ undo: null });
    } catch {
      if (this.current(generation)) {
        this.publish({ undo: null, error: 'undo' });
        try {
          await this.port.refresh();
        } catch {}
      }
    } finally {
      if (this.current(generation)) this.publish({ pending: null });
    }
  }
}
