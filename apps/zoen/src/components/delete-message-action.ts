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
    state.snapshot?.conversations.some((chat) => chat.id === space) === true &&
    Boolean(entry && messageDeletionTargets([entry], state.snapshot?.account?.identity_id, true))
  );
}

/** Single-message menus use the same reviewed command as selected messages. */
export function deleteMessageAction(
  store: ClientStore,
  space: string,
  resolve: () => TimelineEntry | undefined,
  choose: Dialogs['choose'],
  signal?: AbortSignal,
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
    canDeleteEveryone: () =>
      store
        .getSnapshot()
        .snapshot?.conversations.some((chat) => chat.id === space && chat.can_write) === true,
    signal,
    confirm: (count, everyone, activeSignal) =>
      chooseMessageDeletion(choose, count, everyone, activeSignal),
    send: (selection) => store.request({ op: 'delete_messages', ...selection }),
  });
}

export async function chooseMessageDeletion(
  choose: Dialogs['choose'],
  count: number,
  everyone: boolean,
  signal: AbortSignal,
) {
  const value = await choose({
    title: t('chat.deleteSelectedMessages', { count }),
    choices: [
      { id: 'for_me', label: t('chat.deleteOnThisDevice'), destructive: true },
      ...(everyone
        ? [{ id: 'everyone', label: t('chat.deleteForEveryone'), destructive: true }]
        : []),
    ],
    signal,
  });
  return value === 'for_me' || (everyone && value === 'everyone') ? value : null;
}

/** Bind one native menu activation; late callbacks cannot acquire a newer session. */
export function bindMessageDeletion(
  store: ClientStore,
  space: string,
  entry: TimelineEntry,
  choose: Dialogs['choose'],
  action: {
    epoch: number;
    pending: { current: boolean };
    mounted: { current: boolean };
    activation: { current: number };
    confirmation: { current: AbortController | null };
    resolve(entry: TimelineEntry): TimelineEntry | undefined;
    setBusy(value: boolean): void;
  },
) {
  return () => {
    if (action.pending.current || store.sessionEpoch !== action.epoch || !action.resolve(entry))
      return;
    const activation = action.activation.current;
    const controller = new AbortController();
    action.confirmation.current = controller;
    action.pending.current = true;
    action.setBusy(true);
    void deleteMessageAction(
      store,
      space,
      () => (action.activation.current === activation ? action.resolve(entry) : undefined),
      choose,
      controller.signal,
    )
      .catch(() => {
        // The store owns failure presentation; a retry requires another explicit choice.
      })
      .finally(() => {
        if (action.confirmation.current === controller) action.confirmation.current = null;
        action.pending.current = false;
        if (action.mounted.current) action.setBusy(false);
      });
  };
}
