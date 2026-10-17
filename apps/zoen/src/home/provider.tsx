import {
  createContext,
  use,
  useEffect,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from 'react';
import { useClient } from '../core/provider';
import type { ClientState } from '../core/store';
import { t } from '../i18n';
import { preferences } from '../preferences';
import { HomeController, homeOwnerKey, type HomeOwner } from './preferences';

const Context = createContext<HomeController | null>(null);
function ownerFor(state: ClientState): HomeOwner | null {
  if (state.phase !== 'ready' || state.erasePending || !state.snapshot) return null;
  const account = state.snapshot.account;
  if (state.mode === 'demo')
    return {
      mode: 'demo',
      identityId: state.snapshot.me?.id ?? 'demo',
      deviceId: 'demo',
    };
  return account?.unlocked && !account.setup_pending
    ? { mode: 'account', identityId: account.identity_id, deviceId: account.device_id }
    : null;
}

export function HomeProvider({ children }: { children: ReactNode }) {
  const { store } = useClient();
  const [controller] = useState(
    () =>
      new HomeController(
        {
          owner: () => ownerFor(store.getSnapshot()),
          spaces: () => {
            // Snapshot omits Personal spaces. Until core exposes their authenticated scope,
            // Home includes only conversations where the current identity is still a member.
            // Unknown cached spaces must never become implicit Home access grants.
            const snapshot = store.getSnapshot().snapshot;
            const me = snapshot?.me?.id ?? snapshot?.account?.identity_id;
            return new Set(
              snapshot?.conversations
                .filter((space) => space.members.some((member) => member.id === me))
                .map((space) => space.id) ?? [],
            );
          },
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
        homeOwnerKey(ownerFor(store.getSnapshot())) !== homeOwnerKey(controller.getSnapshot().owner)
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

export function useHome() {
  const controller = use(Context);
  if (!controller) throw new Error('HomeProvider is missing.');
  const client = useClient();
  const state = useSyncExternalStore(
    controller.subscribe,
    controller.getSnapshot,
    controller.getSnapshot,
  );
  const current = homeOwnerKey(ownerFor(client)) === homeOwnerKey(state.owner);
  const snapshot = client.snapshot;
  const me = snapshot?.me?.id ?? snapshot?.account?.identity_id;
  const spaces = new Set(
    snapshot?.conversations
      .filter((space) => space.members.some((member) => member.id === me))
      .map((space) => space.id),
  );
  const items = current ? state.items.filter((item) => spaces.has(item.space_id)) : [];
  const pinnedIds = current ? state.pinnedIds : [];
  const byId = new Map(items.map((item) => [item.id, item]));
  const pinnedItems = pinnedIds.flatMap((id) => {
    const item = byId.get(id);
    return item ? [item] : [];
  });
  const error = current ? state.error : null;
  const owner = current ? state.owner : null;
  return {
    items,
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
    isPinned: (id: string) => pinnedIds.includes(id),
  };
}
