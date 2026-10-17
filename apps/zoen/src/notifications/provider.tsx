import {
  createContext,
  use,
  useEffect,
  useMemo,
  useSyncExternalStore,
  type ReactNode,
} from 'react';
import { AppState, Platform } from 'react-native';
import { router } from 'expo-router';
import { useClient } from '../core/provider';
import { PushCoordinator } from './coordinator';
import { validLocalNotification } from './links';
import { decodeWake } from './wake';
import { localNotes } from './local';
import {
  clearDeviceNotifications,
  installNotificationHandler,
  listenNotificationEvents,
  listenPushToken,
  nativePushConfiguration,
  nativePushToken,
  notificationPermission,
  showLocalNote,
  updateBadge,
} from './system';

const Context = createContext<PushCoordinator | null>(null);

export function NotificationProvider({ children }: { children: ReactNode }) {
  const { store, snapshot, mode } = useClient();
  const deviceId = mode === 'account' ? (snapshot?.account?.device_id ?? null) : null;
  const coordinator = useMemo(
    () =>
      new PushCoordinator(
        {
          current: () =>
            Boolean(
              deviceId &&
              store.getSnapshot().mode === 'account' &&
              store.getSnapshot().snapshot?.account?.device_id === deviceId,
            ),
          permission: notificationPermission,
          token: nativePushToken,
          async register(configuration, token) {
            if (store.getSnapshot().snapshot?.account?.device_id !== deviceId)
              throw new Error('Account changed');
            await store.driver.request({
              op: 'register_push',
              platform: configuration.environment === 'sandbox' ? 'apns-sandbox' : 'apns',
              token,
            });
            return { kind: 'acknowledged' };
          },
        },
        Platform.OS === 'ios' ? nativePushConfiguration() : null,
      ),
    [store, deviceId],
  );

  useEffect(() => {
    coordinator.activate();
    if (!deviceId || Platform.OS !== 'ios') return () => coordinator.dispose();
    const current = () =>
      store.getSnapshot().mode === 'account' &&
      store.getSnapshot().snapshot?.account?.device_id === deviceId;
    const canOpen = (data: unknown) => {
      if (!current()) return null;
      const target = validLocalNotification(data, deviceId);
      const currentSnapshot = store.getSnapshot().snapshot;
      const chat =
        target && currentSnapshot?.conversations.find((value) => value.id === target.spaceId);
      return chat &&
        currentSnapshot?.account &&
        chat.members.some((member) => member.id === currentSnapshot.account?.identity_id)
        ? chat
        : null;
    };
    const clearHandler = installNotificationHandler((data) => {
      const chat = canOpen(data);
      return Boolean(chat && !chat.muted && !chat.archived);
    });
    const stopToken = listenPushToken((token) => {
      void coordinator.receivedToken(token);
    });
    const stopEvents = listenNotificationEvents(
      (payload) => {
        const wake = decodeWake(payload);
        if (!wake || !current()) return;
        // A wake only asks the already-authenticated core for its current projection.
        // Membership, decryption and signed-log validation remain inside Rust sync.
        void store.refresh();
      },
      (data) => {
        const chat = canOpen(data);
        if (chat) router.push({ pathname: '/chat/[id]', params: { id: chat.id } });
      },
    );
    let before = store.getSnapshot().snapshot;
    const stopStore = store.subscribe(() => {
      const after = store.getSnapshot().snapshot;
      if (!current() || !after?.connection.synced) return;
      if (
        before &&
        AppState.currentState !== 'active' &&
        coordinator.getSnapshot().permission === 'allowed'
      ) {
        for (const note of localNotes(before, after)) void showLocalNote(note).catch(() => {});
      }
      before = after;
      void updateBadge(
        after.conversations
          .filter((chat) => !chat.muted && !chat.archived)
          .reduce((total, chat) => total + chat.unread, 0),
      ).catch(() => {});
    });
    const foreground = AppState.addEventListener('change', (state) => {
      if (state === 'active') void coordinator.foreground();
    });
    void coordinator.foreground();
    return () => {
      stopStore();
      stopEvents();
      stopToken();
      clearHandler();
      foreground.remove();
      coordinator.dispose();
      if (!current()) void clearDeviceNotifications(deviceId).catch(() => {});
    };
  }, [store, deviceId, coordinator]);
  return <Context value={coordinator}>{children}</Context>;
}

export function useNotifications() {
  const coordinator = use(Context);
  if (!coordinator) throw new Error('NotificationProvider is missing.');
  return {
    ...useSyncExternalStore(
      coordinator.subscribe,
      coordinator.getSnapshot,
      coordinator.getSnapshot,
    ),
    requestPermission: () => coordinator.foreground(true),
    refresh: () => coordinator.foreground(),
  };
}
