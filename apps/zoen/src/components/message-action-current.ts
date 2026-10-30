import { t } from '../i18n/index.ts';
import { homeSpacesFor } from '../home/access';
import { messageCanInteract, messageCanSelect } from '../core/message-presentation';
import type { ClientStore } from '../core/store';
import type { TimelineEntry } from '../core/types';

export function currentMessageAction(
  store: ClientStore,
  space: string,
  scope: string,
  focused: boolean,
  latest: { entries: readonly TimelineEntry[]; disabled: boolean; scope: string },
  entry: TimelineEntry,
  purpose: 'content' | 'delete' | 'thread' = 'content',
): TimelineEntry | undefined {
  const state = store.getSnapshot();
  if (
    !focused ||
    store.isSessionRetiring ||
    latest.disabled ||
    latest.scope !== scope ||
    state.phase !== 'ready' ||
    !homeSpacesFor(state).has(space) ||
    `${state.mode}:${state.snapshot?.account?.identity_id ?? state.snapshot?.me?.id}:${state.snapshot?.account?.device_id}:${space}` !==
      scope
  )
    return undefined;
  const value = latest.entries.find((value) => value.id === entry.id);
  if (value?.deleted !== entry.deleted || value?.text_revision !== entry.text_revision)
    return undefined;
  return value &&
    (messageCanInteract(value) ||
      (purpose === 'delete' && messageCanSelect(value)) ||
      (purpose === 'thread' &&
        value.deleted &&
        'Message' in value.kind &&
        !value.in_thread &&
        value.thread_replies > 0))
    ? value
    : undefined;
}

/** Resolve the current signed author; a recycled row or late response grants no navigation. */
export async function resolveMessageAuthor(
  store: ClientStore,
  space: string,
  message: string,
  active: () => boolean,
): Promise<string | null> {
  const canonical = await store.driver.request({
    op: 'message',
    space_id: space,
    message_id: message,
  });
  if (!active()) return null;
  if (canonical.id !== message || !messageCanInteract(canonical) || !canonical.author.id.trim())
    throw new Error(t('chat.thisMessageIsNoLongerInThisDevice'));
  return canonical.author.id;
}
