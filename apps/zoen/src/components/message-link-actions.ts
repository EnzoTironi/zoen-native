import * as Clipboard from 'expo-clipboard';
import { conversationScope } from '../core/conversation-scope';
import { messageAddress } from '../notifications/links';
import { t } from '../i18n/index.ts';
import type { ClientStore } from '../core/store';
import type { TimelineEntry } from '../core/types';

/** Menu callbacks retain the opening epoch even when UIKit dismisses later. */
export async function copyMessageLink(
  store: ClientStore,
  space: string,
  entry: TimelineEntry,
  current: (entry: TimelineEntry) => TimelineEntry | undefined,
  epoch: number,
  inform: (title: string, message: string) => Promise<void>,
): Promise<void> {
  const owner = conversationScope(store.getSnapshot(), epoch, space);
  const eligible = () =>
    !store.isSessionRetiring &&
    store.getSnapshot().mode === 'account' &&
    epoch === store.sessionEpoch &&
    owner &&
    conversationScope(store.getSnapshot(), store.sessionEpoch, space) === owner &&
    current(entry);
  const value = eligible();
  const address =
    value && !value.deleted && 'Message' in value.kind ? messageAddress(space, value.id) : null;
  if (!address) return;
  let accepted = false;
  try {
    accepted = (await Clipboard.setStringAsync(address)) === true;
  } catch {
    /* Native throws and rejected promises share the visible failure. */
  }
  if (!accepted && eligible()) await inform(t('chat.copyLinkFailed'), t('chat.tryAgain'));
}
