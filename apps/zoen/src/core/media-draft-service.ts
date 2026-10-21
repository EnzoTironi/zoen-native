import { homeOwnerFor, homeSpacesFor } from '../home/access.ts';
import { MediaDraftChannel, MediaDraftFailure } from '../media/media-draft-channel.ts';
import type { MediaDraftScope } from '../media/media-draft-types.ts';
import type { ClientStore } from './store.ts';
import { NormalDraftFailure } from './normal-message-drafts.ts';
import { t } from '../i18n/index.ts';

/** Session-owned channels only; no native work is started by render-time lookup. */
export class MediaDraftService {
  private readonly store: ClientStore;
  private session: { key: string; epoch: number } | null = null;
  private readonly channels = new Map<string, MediaDraftChannel>();
  private unregister: (() => void) | undefined;
  private retiring = false;
  constructor(store: ClientStore) {
    this.store = store;
  }

  channel(spaceId: string, threadRootId: string | null): MediaDraftChannel | null {
    if (this.store.isSessionRetiring) return null;
    const state = this.store.getSnapshot();
    const owner = homeOwnerFor(state);
    const port = this.store.driver.media?.drafts;
    if (!port || owner?.mode !== 'account' || !homeSpacesFor(state).has(spaceId) || this.retiring)
      return null;
    const key = JSON.stringify([owner.identityId, owner.deviceId, this.store.sessionEpoch]);
    if (this.session && this.session.key !== key) return null;
    if (!this.session) {
      this.session = { key, epoch: this.store.sessionEpoch };
      this.unregister = this.store.registerSessionCleanup((discard) => this.retire(discard));
    }
    const scope: MediaDraftScope = {
      expected_identity_id: owner.identityId,
      expected_device_id: owner.deviceId,
      space_id: spaceId,
      thread_root_id: threadRootId,
    };
    const scopeKey = JSON.stringify(scope);
    let channel = this.channels.get(scopeKey);
    if (!channel) {
      const created: MediaDraftChannel = new MediaDraftChannel(scope, port, (): boolean =>
        this.isCurrent(created),
      );
      channel = created;
      this.channels.set(scopeKey, channel);
    }
    return channel;
  }

  isCurrent(channel: MediaDraftChannel): boolean {
    const state = this.store.getSnapshot();
    const owner = homeOwnerFor(state);
    const scope = channel.scope;
    return (
      !this.retiring &&
      this.session?.epoch === this.store.sessionEpoch &&
      this.channels.get(JSON.stringify(scope)) === channel &&
      owner?.mode === 'account' &&
      owner.identityId === scope.expected_identity_id &&
      owner.deviceId === scope.expected_device_id &&
      homeSpacesFor(state).has(scope.space_id)
    );
  }

  async recoverFailed(
    spaceId: string,
    threadRootId: string | null,
    messageId: string,
    current: () => boolean,
  ): Promise<void> {
    const media = this.channel(spaceId, threadRootId);
    const normal = this.store.messageDrafts.channel(spaceId, threadRootId);
    if (!media || !normal || !this.store.driver.media?.drafts?.recoverFailed)
      throw new Error(t('media.recoveryUnavailable'));
    try {
      await normal.controller.withEmptyDraft(normal.scope, (revision, unchanged) =>
        media.recoverFailed(
          messageId,
          revision,
          () =>
            current() &&
            unchanged() &&
            this.isCurrent(media) &&
            this.store.messageDrafts.isCurrent(normal),
        ),
      );
    } catch (cause) {
      if (
        (cause instanceof NormalDraftFailure || cause instanceof MediaDraftFailure) &&
        (cause.kind === 'conflict' || cause.kind === 'busy')
      )
        throw new Error(t('media.recoveryDraftOccupied'), { cause });
      throw cause;
    }
  }

  private async retire(discard: boolean): Promise<void> {
    const session = this.session;
    if (!session) return;
    this.retiring = true;
    // Calling retire synchronously freezes every channel before Store publishes
    // another phase/account. Erasure cancels queued writes before draining native IO.
    const results = await Promise.allSettled(
      [...this.channels.values()].map((channel) => channel.retire(discard)),
    );
    const failure = results.find((result) => result.status === 'rejected');
    if (failure?.status === 'rejected') throw failure.reason;
    if (this.session !== session) return;
    this.channels.clear();
    this.session = null;
    this.retiring = false;
    this.unregister?.();
    this.unregister = undefined;
  }
}
