import {
  createContext,
  use,
  useCallback,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import { AppState } from 'react-native';
import { useFocusEffect } from 'expo-router';
import type { ClientStore } from '../core/store.ts';
import { homeOwnerFor, homeSpacesFor } from '../home/access.ts';
import type { TimelineEntry } from '../core/types.ts';
import { mediaPlayback } from './playback-owner.ts';
import { createMessageAudioRuntime } from './message-audio-runtime.ts';
import {
  MessageAudioSession,
  type MessageAudioScope,
  type MessageAudioSource,
} from './message-audio-session.ts';

const Context = createContext<MessageAudioSession | null>(null);

function createSession(store: ClientStore) {
  const scope = (): MessageAudioScope | null => {
    const owner = homeOwnerFor(store.getSnapshot());
    return !store.isSessionRetiring && owner?.mode === 'account'
      ? { identityId: owner.identityId, deviceId: owner.deviceId, epoch: store.sessionEpoch }
      : null;
  };
  return new MessageAudioSession({
    coordinator: mediaPlayback,
    scope,
    authorized: (source) => {
      const owner = scope();
      return (
        !!owner &&
        owner.identityId === source.scope.identityId &&
        owner.deviceId === source.scope.deviceId &&
        owner.epoch === source.scope.epoch &&
        homeSpacesFor(store.getSnapshot()).has(source.spaceId)
      );
    },
    verify: async (source) => {
      // Raw read-only requests do not trigger ClientStore.refresh or recursive subscriptions.
      const [message, item] = await Promise.all([
        store.driver.request({
          op: 'message',
          space_id: source.spaceId,
          message_id: source.messageId,
        }),
        store.driver.request({ op: 'item', item_id: source.itemId }),
      ]);
      if (
        message.id !== source.messageId ||
        message.deleted ||
        message.voice?.item_id !== source.itemId ||
        message.voice.sha256 !== source.sha256 ||
        item.id !== source.itemId ||
        item.space_id !== source.spaceId ||
        item.file?.sha256 !== source.sha256 ||
        !item.file.mime.startsWith('audio/')
      )
        throw new Error('The audio message changed.');
      return item.version;
    },
    materialize: async (source, version) => {
      const media = store.driver.media;
      if (!media) throw new Error('Audio playback is unavailable.');
      return media.materializeFile(source.itemId, version);
    },
    release: async (file) => {
      const media = store.driver.media;
      if (!media) throw new Error('Audio cleanup is unavailable.');
      await media.releaseLocalFile(file.uri);
    },
    create: createMessageAudioRuntime,
  });
}

/** One native account owner; deferred platforms retain their existing media behavior. */
export function MessageAudioProvider({
  store,
  children,
}: {
  store: ClientStore;
  children: ReactNode;
}) {
  const [session] = useState(() => (process.env.EXPO_OS === 'ios' ? createSession(store) : null));
  useEffect(() => {
    if (!session) return;
    let previous = store.getSnapshot();
    const unsubscribe = store.subscribe(() => {
      const next = store.getSnapshot();
      const canonicalChanged =
        next.snapshot !== previous.snapshot || next.timelines !== previous.timelines;
      previous = next;
      session.reconcile();
      // Local offline edits/deletes also publish canonical projections without a native change event.
      if (canonicalChanged) session.changed();
    });
    const stopEvents = store.driver.subscribe((event) => {
      if (event.kind === 'change') session.changed(event.spaceIds);
    });
    const unregisterSession = store.registerSessionCleanup(() => session.retireSession());
    const unregisterDevice = store.registerDeviceCleanup(() => session.retireSession());
    const appState = AppState.addEventListener('change', (state) =>
      session.setForeground(state === 'active'),
    );
    void session.activate().catch(() => {});
    session.setForeground(AppState.currentState === 'active');
    session.reconcile();
    return () => {
      unsubscribe();
      stopEvents();
      appState.remove();
      // Keep drain registrations alive if teardown fails; a subsequent close/erase retries it.
      void session
        .dispose()
        .then(() => {
          unregisterSession();
          unregisterDevice();
        })
        .catch(() => {});
    };
  }, [store, session]);
  return <Context value={session}>{children}</Context>;
}

export function useMessageAudioFocus(spaceId: string, owner: string) {
  const session = use(Context);
  const target = useMemo(() => ({ spaceId, owner }), [spaceId, owner]);
  useFocusEffect(useCallback(() => session?.focus(target.spaceId), [session, target]));
}

export function useReceivedMessageAudio(
  spaceId: string,
  entry: TimelineEntry | null,
  scope: MessageAudioScope | null,
) {
  const session = use(Context);
  const source =
    entry?.voice && !entry.deleted && scope
      ? {
          scope,
          spaceId,
          messageId: entry.id,
          itemId: entry.voice.item_id,
          sha256: entry.voice.sha256,
          durationMs: Number(entry.voice.ms),
        }
      : null;
  const encoded = JSON.stringify(source);
  return useMemo(() => {
    const captured = JSON.parse(encoded) as MessageAudioSource | null;
    return session && captured ? session.channel(captured) : null;
  }, [session, encoded]);
}
