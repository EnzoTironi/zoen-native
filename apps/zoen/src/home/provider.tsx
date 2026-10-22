import {
  createContext,
  use,
  useEffect,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from 'react';
import { useClient } from '../core/provider';
import { t } from '../i18n';
import { preferences } from '../preferences';
import { HomeController, homeOwnerKey, projectHomeItems } from './preferences';
import { homeOwnerFor, homeSpacesFor } from './access';

const Context = createContext<HomeController | null>(null);

export function HomeProvider({ children }: { children: ReactNode }) {
  const { store } = useClient();
  const [controller] = useState(
    () =>
      new HomeController(
        {
          owner: () => homeOwnerFor(store.getSnapshot()),
          revision: () =>
            JSON.stringify([store.sessionEpoch, store.getSnapshot().snapshot?.account?.relay_url]),
          spaces: () => homeSpacesFor(store.getSnapshot()),
          items: () => store.driver.request({ op: 'items' }),
        },
        preferences,
      ),
  );
  useEffect(() => {
    let snapshot = store.getSnapshot().snapshot;
    const unsubscribe = store.subscribe(() => {
      const next = store.getSnapshot().snapshot;
      // Presence, typing and timeline-only changes do not reread every miniapp.
      if (
        next !== snapshot ||
        homeOwnerKey(homeOwnerFor(store.getSnapshot())) !==
          homeOwnerKey(controller.getSnapshot().owner)
      ) {
        snapshot = next;
        void controller.refresh();
      }
    });
    const unregister = store.registerDeviceCleanup(() => controller.eraseDevice());
    void controller.activate();
    return () => {
      unsubscribe();
      unregister();
      controller.dispose();
    };
  }, [store, controller]);
  return <Context value={controller}>{children}</Context>;
}

export function useHomeController() {
  const controller = use(Context);
  if (!controller) throw new Error('HomeProvider is missing.');
  return controller;
}

export function useHome() {
  const controller = useHomeController();
  const client = useClient();
  const state = useSyncExternalStore(
    controller.subscribe,
    controller.getSnapshot,
    controller.getSnapshot,
  );
  const current = homeOwnerKey(homeOwnerFor(client)) === homeOwnerKey(state.owner);
  const spaces = homeSpacesFor(client);
  const { items, allItems, hiddenItems } = current
    ? projectHomeItems(
        state.items.filter((item) => spaces.has(item.space_id)),
        state,
      )
    : { items: [], allItems: [], hiddenItems: [] };
  const pinnedIds = current ? state.pinnedIds : [];
  const byId = new Map(items.map((item) => [item.id, item]));
  const pinnedItems = pinnedIds.flatMap((id) => {
    const item = byId.get(id);
    return item ? [item] : [];
  });
  const error = current ? state.error : null;
  const owner = current ? state.owner : null;
  return {
    ownerKey: homeOwnerKey(owner),
    editing: current && state.editing,
    editSaving: current && state.editSaving,
    beginEdit: () => controller.beginEdit(owner),
    requireEdit: () => controller.requireEdit(owner),
    discardEdit: () => controller.discardEdit(owner),
    commitEdit: () => controller.commitEdit(owner),
    items,
    allItems,
    hiddenItems,
    pinnedItems,
    pinnedIds,
    loading: !current || state.loading,
    error: error
      ? error.cause instanceof Error
        ? error.cause.message
        : t(
            error.kind === 'save'
              ? 'home.couldNotSavePins'
              : error.kind === 'accountChanged'
                ? 'home.accountChanged'
                : 'home.couldNotLoadMiniapps',
          )
      : null,
    pendingRequestCount:
      client.snapshot?.requests.filter(
        (request) => request.status === 'Pending' || request.status === 'Stale',
      ).length ?? 0,
    retry: () => {
      void controller.refresh();
    },
    // A menu presented by a previous account cannot act on the next account's shared item.
    pin: (id: string) => controller.pin(id, owner),
    unpin: (id: string) => controller.unpin(id, owner),
    hide: (id: string) => controller.hide(id, owner),
    restore: (id: string) => controller.restore(id, owner),
    restoreAll: () => controller.restoreAll(owner),
    reorder: (ids: readonly string[], expectedIds?: readonly string[]) =>
      controller.reorder(ids, owner, expectedIds),
    reorderItems: (ids: readonly string[], expectedIds?: readonly string[]) =>
      controller.reorderItems(ids, owner, expectedIds),
    isPinned: (id: string) => pinnedIds.includes(id),
  };
}
