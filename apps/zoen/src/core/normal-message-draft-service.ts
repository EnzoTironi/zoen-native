import { homeOwnerFor, homeSpacesFor } from '../home/access.ts';
import {
  NormalMessageDrafts,
  type NormalDraftScope,
  type NormalDraftSnapshot,
} from './normal-message-drafts.ts';
import { sameEdit } from './normal-message-draft-types.ts';
import type { ClientStore } from './store.ts';

type DraftValue = NormalDraftSnapshot;
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
    if (this.store.isSessionRetiring) return null;
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
    const receipts = new Map<
      string,
      { revision: string; message_id: string; text_revision: string | null }
    >();
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
      replyIntent: (scope) =>
        native(scope) ? (this.store.driver.localIntentId?.() ?? null) : null,
      load: async (scope) =>
        native(scope)
          ? this.store.driver.request({ op: 'message_draft', ...target(scope) })
          : read(scope),
      save: async (
        scope,
        expectedRevision,
        text,
        replyTo,
        replyQuote,
        sharedReply,
        edit,
        replyIntent,
      ) => {
        if (native(scope))
          return this.store.driver.request({
            op: 'save_message_draft',
            ...target(scope),
            expected_revision: expectedRevision,
            text,
            reply_to: replyTo,
            reply_quote: replyQuote ?? null,
            shared_reply: sharedReply ?? undefined,
            edit: edit ?? undefined,
            ...(replyIntent ? { reply_intent: replyIntent } : {}),
          });
        if (replyQuote || sharedReply)
          throw new Error('Quoted replies require the signed-in native account.');
        const current = read(scope);
        if (current.revision !== expectedRevision) throw new Error('Draft changed.');
        if (current.text === text && current.reply_to === replyTo && sameEdit(current.edit, edit))
          return current;
        const next = {
          revision: String(++revision),
          text,
          reply_to: replyTo,
          ...(edit ? { edit } : {}),
        };
        volatile.set(scopeKey(scope), next);
        return next;
      },
      sendEdit: async (scope, expectedRevision) => {
        if (native(scope))
          return this.store.driver.request({
            op: 'send_message_edit_draft',
            ...target(scope),
            expected_revision: expectedRevision,
          });
        const key = scopeKey(scope),
          receipt = receipts.get(key);
        if (receipt?.revision === expectedRevision)
          return {
            message_id: receipt.message_id,
            text_revision: receipt.text_revision,
            entry: null,
            draft: read(scope),
          };
        const current = read(scope),
          edit = current.edit;
        if (current.revision !== expectedRevision || !edit) throw new Error('Draft changed.');
        await this.store.driver.request({
          op: 'edit_message',
          expected_identity_id: scope.owner.identityId,
          expected_device_id: scope.owner.deviceId,
          space_id: scope.spaceId,
          message_id: edit.message_id,
          expected_revision: edit.source_revision,
          text: edit.text,
        });
        const { edit: _edit, ...rest } = current;
        const draft = { ...rest, revision: String(++revision) };
        volatile.set(key, draft);
        receipts.set(key, {
          revision: expectedRevision,
          message_id: edit.message_id,
          text_revision: null,
        });
        return { message_id: edit.message_id, text_revision: null, entry: null, draft };
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
          return {
            message_id: receipt.message_id,
            text_revision: receipt.text_revision,
            entry: null,
            draft: read(scope),
          };
        const current = read(scope);
        if (current.revision !== expectedRevision) throw new Error('Draft changed.');
        const entry = await this.store.driver.request({
          op: 'send_message',
          space_id: scope.spaceId,
          text: current.text,
          reply_to: current.reply_to ?? scope.threadRootId,
          thread: Boolean(scope.threadRootId),
        });
        const draft = {
          revision: String(++revision),
          text: '',
          reply_to: null,
          ...(current.edit ? { edit: current.edit } : {}),
        };
        volatile.set(key, draft);
        receipts.set(key, {
          revision: expectedRevision,
          message_id: entry.id,
          text_revision: entry.text_revision ?? null,
        });
        return { message_id: entry.id, text_revision: entry.text_revision ?? null, entry, draft };
      },
    });
  }
}
