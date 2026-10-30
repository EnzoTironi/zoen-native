import { homeSpacesFor } from '../home/access';
import { messageCanInteract } from '../core/message-presentation';
import type { ClientStore } from '../core/store';
import type { TimelineEntry } from '../core/types';

export function currentMessageAction(
  store: ClientStore,
  space: string,
  scope: string,
  focused: boolean,
  latest: { entries: readonly TimelineEntry[]; disabled: boolean; scope: string },
  entry: TimelineEntry,
  deletedThreadRoot = false,
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
  return value &&
    (messageCanInteract(value) ||
      (deletedThreadRoot &&
        value.deleted &&
        'Message' in value.kind &&
        !value.in_thread &&
        value.thread_replies > 0))
    ? value
    : undefined;
}
