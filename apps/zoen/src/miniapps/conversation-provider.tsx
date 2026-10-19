import {
  createContext,
  use,
  useEffect,
  useMemo,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from 'react';
import { useClient } from '../core/provider';
import { useHomeController } from '../home/provider';
import { homeOwnerFor, homeSpacesFor } from '../home/access';
import { homeOwnerKey } from '../home/preferences';
import { t } from '../i18n';
import { preferences } from '../preferences';
import {
  ConversationCardsController,
  conversationOwnerKey,
  projectConversationCards,
  type ConversationCards,
} from './conversation-preferences';

const Context = createContext<ConversationCardsController | null>(null);
const EMPTY_CARDS: ConversationCards = { all: [], visible: [], hiddenIds: [] };

export function ConversationCardsProvider({ children }: { children: ReactNode }) {
  const { store } = useClient();
  const home = useHomeController();
  const [controller] = useState(
    () =>
      new ConversationCardsController(
        {
          owner: () => homeOwnerFor(store.getSnapshot()),
          spaces: () => homeSpacesFor(store.getSnapshot()),
          // Home already reads and validates canonical miniapps. Reuse that source;
          // conversation choices never create another content cache or core query.
          items: () =>
            homeOwnerKey(home.getSnapshot().owner) ===
            homeOwnerKey(homeOwnerFor(store.getSnapshot()))
              ? home.getSnapshot().items
              : [],
        },
        preferences,
      ),
  );
  useEffect(() => {
    const unsubscribeClient = store.subscribe(() => {
      void controller.refresh();
    });
    const unsubscribeHome = home.subscribe(() => {
      void controller.refresh();
    });
    const unregister = store.registerDeviceCleanup(() => controller.eraseDevice());
    void controller.activate();
    return () => {
      unsubscribeClient();
      unsubscribeHome();
      unregister();
      controller.dispose();
    };
  }, [store, home, controller]);
  return <Context value={controller}>{children}</Context>;
}

export function useConversationCards(spaceId: string) {
  const controller = use(Context);
  if (!controller) throw new Error('ConversationCardsProvider is missing.');
  const client = useClient();
  const home = useHomeController();
  const canonical = useSyncExternalStore(home.subscribe, home.getSnapshot, home.getSnapshot);
  const state = useSyncExternalStore(
    controller.subscribe,
    controller.getSnapshot,
    controller.getSnapshot,
  );
  const currentOwner = homeOwnerFor(client);
  const current = conversationOwnerKey(currentOwner) === conversationOwnerKey(state.owner);
  const canonicalCurrent = homeOwnerKey(currentOwner) === homeOwnerKey(canonical.owner);
  const owner = current ? state.owner : null;
  const { snapshot, phase, mode, erasePending } = client;
  const spaces = useMemo(
    () => homeSpacesFor({ snapshot, phase, mode, erasePending }),
    [snapshot, phase, mode, erasePending],
  );
  const preference = state.preferences[spaceId];
  const canProject =
    current && canonicalCurrent && !!state.owner && !state.loading && state.error?.kind !== 'load';
  // Render only from subscribed immutable inputs. Reading controller internals
  // here lets React Compiler reuse stale cards after a successful preference save.
  const cards = useMemo(
    () =>
      canProject
        ? projectConversationCards(canonical.items, spaces, spaceId, preference)
        : EMPTY_CARDS,
    [canonical.items, spaces, spaceId, preference, canProject],
  );
  const pinnedIds = useMemo(() => cards.visible.map((item) => item.id), [cards.visible]);
  const canonicalError = canonicalCurrent && canonical.error?.kind === 'load';
  const error = current ? state.error : null;
  return {
    ownerKey: conversationOwnerKey(owner),
    available: spaces.has(spaceId),
    items: cards.all,
    visible: cards.visible,
    pinnedIds,
    loading:
      !current ||
      !canonicalCurrent ||
      state.loading ||
      (canonical.loading && canonical.items.length === 0),
    error: canonicalError
      ? t('conversationCards.couldNotLoad')
      : error
        ? t(
            error.kind === 'save'
              ? 'conversationCards.couldNotSave'
              : error.kind === 'accountChanged'
                ? 'conversationCards.accountChanged'
                : 'conversationCards.couldNotLoad',
          )
        : null,
    retryable: !!canonicalError || error?.kind === 'load',
    isPinned: (id: string) => cards.visible.some((item) => item.id === id),
    setPinned: (id: string, pinned: boolean) => controller.setPinned(spaceId, id, pinned, owner),
    reorder: (ids: readonly string[]) => controller.reorder(spaceId, ids, owner),
    retry: () => {
      if (canonicalError) void home.refresh();
      void controller.refresh();
    },
  };
}
