import { homeOwnerFor, homeSpacesFor } from '../home/access.ts';
import { NormalMessageDrafts, type NormalDraftScope } from './normal-message-drafts.ts';
import type { ClientStore } from './store.ts';
import type { TimelineEntry } from './types.ts';

type DraftValue = { revision: string; text: string; reply_to: string | null };
export type NormalDraftChannel = Readonly<{
  scope: NormalDraftScope;
  controller: NormalMessageDrafts;
}>;

const scopeKey = (scope: NormalDraftScope) => JSON.stringify(scope);

/** A session owns its transient controllers; Rust alone persists native account drafts. */
export class NormalMessageDraftService {
  private session: { key: string; controller: NormalMessageDrafts } | null = null;
  private readonly channels = new Map<string, NormalDraftChannel>();
  private unregister: (() => void) | undefined;
  private readonly store: ClientStore;
  constructor(store: ClientStore) {
    this.store = store;
  }

  channel(spaceId: string, threadRootId: string | null): NormalDraftChannel | null {
    const state = this.store.getSnapshot();
    const owner = homeOwnerFor(state);
    if (!owner || !homeSpacesFor(state).has(spaceId)) return null;
    const scope: NormalDraftScope = {
      owner: { ...owner, sessionEpoch: this.store.sessionEpoch },
      spaceId,
      threadRootId,
    };
    const key = JSON.stringify(scope.owner);
    if (!this.session || this.session.key !== key) {
      if (this.session) return null; // Retirement must finish before another owner is admitted.
      this.session = { key, controller: this.createController() };
      this.unregister = this.store.registerSessionCleanup((discard) => this.retire(discard));
    }
    const id = scopeKey(scope);
    let channel = this.channels.get(id);
    if (!channel) {
      channel = Object.freeze({ scope, controller: this.session.controller });
      this.channels.set(id, channel);
    }
    return channel;
  }

  isCurrent(channel: NormalDraftChannel): boolean {
    const { scope } = channel;
    const state = this.store.getSnapshot();
    const owner = homeOwnerFor(state);
    return (
      this.session?.controller === channel.controller &&
      this.store.sessionEpoch === scope.owner.sessionEpoch &&
      owner?.identityId === scope.owner.identityId &&
      owner.deviceId === scope.owner.deviceId &&
      owner.mode === scope.owner.mode &&
      homeSpacesFor(state).has(scope.spaceId)
    );
  }

  private async retire(discard: boolean): Promise<void> {
    const session = this.session;
    if (!session) return;
    await session.controller.retire({ flush: !discard });
    if (this.session !== session) return;
    this.session = null;
    this.channels.clear();
    this.unregister?.();
    this.unregister = undefined;
  }

  private createController(): NormalMessageDrafts {
    // Deferred Web and demo retain their existing volatile behavior. They never
    // write plaintext drafts to preferences, localStorage or another database.
    const volatile = new Map<string, DraftValue>();
    const receipts = new Map<string, { revision: string; entry: TimelineEntry }>();
    let revision = 0;
    const read = (scope: NormalDraftScope): DraftValue =>
      volatile.get(scopeKey(scope)) ?? { revision: '0', text: '', reply_to: null };
    const native = (scope: NormalDraftScope) =>
      this.store.driver.durableMessageDrafts && scope.owner.mode === 'account';
    const target = (scope: NormalDraftScope) => ({
      expected_identity_id: scope.owner.identityId,
      expected_device_id: scope.owner.deviceId,
      space_id: scope.spaceId,
      thread_root_id: scope.threadRootId,
    });
    return new NormalMessageDrafts({
      load: async (scope) =>
        native(scope)
          ? this.store.driver.request({ op: 'message_draft', ...target(scope) })
          : read(scope),
      save: async (scope, expectedRevision, text, replyTo) => {
        if (native(scope))
          return this.store.driver.request({
            op: 'save_message_draft',
            ...target(scope),
            expected_revision: expectedRevision,
            text,
            reply_to: replyTo,
          });
        const current = read(scope);
        if (current.revision !== expectedRevision) throw new Error('Draft changed.');
        if (current.text === text && current.reply_to === replyTo) return current;
        const next = { revision: String(++revision), text, reply_to: replyTo };
        volatile.set(scopeKey(scope), next);
        return next;
      },
      send: async (scope, expectedRevision) => {
        if (native(scope))
          return this.store.driver.request({
            op: 'send_message_draft',
            ...target(scope),
            expected_revision: expectedRevision,
          });
        const key = scopeKey(scope);
        const receipt = receipts.get(key);
        if (receipt?.revision === expectedRevision)
          return { entry: receipt.entry, draft: read(scope) };
        const current = read(scope);
        if (current.revision !== expectedRevision) throw new Error('Draft changed.');
        const entry = await this.store.driver.request({
          op: 'send_message',
          space_id: scope.spaceId,
          text: current.text,
          reply_to: current.reply_to ?? scope.threadRootId,
          thread: Boolean(scope.threadRootId),
        });
        const draft = { revision: String(++revision), text: '', reply_to: null };
        volatile.set(key, draft);
        receipts.set(key, { revision: expectedRevision, entry });
        return { entry, draft };
      },
    });
  }
}
