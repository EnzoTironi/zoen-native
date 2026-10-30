import { conversationScope } from '../core/conversation-scope.ts';
import { confirmMessageDeletion, messageDeletionTargets } from '../core/message-deletion.ts';
import type { ClientStore } from '../core/store';
import type { TimelineEntry } from '../core/types';
import { t } from '../i18n/index.ts';
import type { Dialogs } from './dialog-types';

export function canDeleteMessage(
  store: ClientStore,
  space: string,
  entry: TimelineEntry | null,
): entry is TimelineEntry {
  const state = store.getSnapshot();
  return (
    state.mode === 'account' &&
    !store.isSessionRetiring &&
    Boolean(conversationScope(state, store.sessionEpoch, space)) &&
    state.snapshot?.conversations.some((chat) => chat.id === space && chat.can_write) === true &&
    Boolean(entry && messageDeletionTargets([entry], state.snapshot?.account?.identity_id))
  );
}

/** Single-message menus use the same reviewed command as selected messages. */
export function deleteMessageAction(
  store: ClientStore,
  space: string,
  resolve: () => TimelineEntry | undefined,
  confirm: Dialogs['confirm'],
): Promise<boolean> {
  const state = store.getSnapshot();
  const account = state.snapshot?.account;
  const scope = conversationScope(state, store.sessionEpoch, space);
  if (state.mode !== 'account' || !account || !scope) return Promise.resolve(false);
  return confirmMessageDeletion({
    identity: account.identity_id,
    device: account.device_id,
    space,
    resolve: () => {
      const live = store.getSnapshot();
      if (conversationScope(live, store.sessionEpoch, space) !== scope) return [];
      const entry = resolve();
      return entry && canDeleteMessage(store, space, entry) ? [entry] : [];
    },
    subscribe: (changed) => store.subscribe(changed),
    confirm: () =>
      confirm({
        title: t('chat.deleteMessage'),
        message: t('chat.theMessageWillBeRemovedForEveryoneIn'),
        confirmLabel: t('chat.delete'),
        destructive: true,
      }),
    send: (selection) => store.request({ op: 'delete_messages', ...selection }),
  });
}
