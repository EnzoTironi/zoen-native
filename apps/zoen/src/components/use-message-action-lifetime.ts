import { useCallback, useLayoutEffect, useMemo, useSyncExternalStore } from 'react';
import { useFocusEffect } from 'expo-router';
import type { ClientState, ClientStore } from '../core/store';
import type { TimelineEntry } from '../core/types';
import { messageActionOwner } from './message-action-current';
import { MessageActionLifetime } from './message-action-lifetime';

export function useMessageActionLifetime({
  store,
  state,
  spaceId,
  threadRootId = null,
  entries,
  disabled,
  setBusy,
  clearSelection,
}: {
  store: ClientStore;
  state: ClientState;
  spaceId: string;
  threadRootId?: string | null;
  entries: readonly TimelineEntry[];
  disabled: boolean;
  setBusy(value: boolean): void;
  clearSelection(): void;
}) {
  const { scope, epoch: infoEpoch } = messageActionOwner(store, state, spaceId);
  const controller = useMemo(
    () => new MessageActionLifetime(store, spaceId, scope, infoEpoch, clearSelection, threadRootId),
    [store, spaceId, scope, infoEpoch, clearSelection, threadRootId],
  );
  const observation = useSyncExternalStore(
    controller.subscribe,
    controller.getSnapshot,
    controller.getSnapshot,
  );
  useLayoutEffect(() => {
    controller.mount();
    return () => controller.unmount();
  }, [controller]);
  useLayoutEffect(() => controller.observe(entries, disabled), [controller, entries, disabled]);
  useFocusEffect(
    useCallback(() => {
      controller.focus();
      setBusy(controller.isPending());
      const unsubscribe = store.subscribe(controller.changed);
      const cleanup = store.registerSessionCleanup(async () => controller.retire());
      return () => {
        unsubscribe();
        cleanup();
        controller.blur();
      };
    }, [store, controller, setBusy]),
  );
  const current = useCallback(
    (entry: TimelineEntry, purpose: 'content' | 'delete' | 'thread' = 'content') =>
      controller.current(observation, entry, purpose),
    [controller, observation],
  );
  return { scope, infoEpoch, controller, current };
}
