import { t } from '../i18n/index.ts';
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
import { useDialogs } from '../components/dialogs';
import { PushCoordinator } from './coordinator';
import { validLocalNotification } from './links';
import { nativeNotificationPlatform } from './native-adapter';
import { pushPlatform } from './configuration';
import { localNotes } from './local';
import { NotificationJobs } from './jobs';
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
  const dialogs = useDialogs();
  const deviceId = mode === 'account' ? (snapshot?.account?.device_id ?? null) : null;
  const jobs = useMemo(() => (deviceId ? new NotificationJobs() : null), [deviceId]);
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
              throw new Error(t('notifications.accountChanged'));
            await store.driver.request({
              op: 'register_push',
              platform: pushPlatform(configuration),
              token,
            });
            return { kind: 'acknowledged' };
          },
        },
        nativePushConfiguration(),
      ),
    [store, deviceId],
  );

  useEffect(
    () =>
      store.registerDeviceCleanup(async () => {
        coordinator.dispose();
        await jobs?.drain();
        if (deviceId) await clearDeviceNotifications(deviceId);
      }),
    [store, coordinator, deviceId, jobs],
  );

  useEffect(() => {
    coordinator.activate();
    if (!deviceId || !jobs || !nativeNotificationPlatform(Platform.OS))
      return () => coordinator.dispose();
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
      (_wake) => {
        if (!current()) return;
        // A wake only asks the already-authenticated core for its current projection.
        // Membership, decryption and signed-log validation remain inside Rust sync.
        void store.refresh();
      },
      (data) => {
        const chat = canOpen(data);
        if (!chat) return;
        try {
          store.assertExternalNavigationAllowed();
        } catch {
          void dialogs.inform(t('drawing.pending'), t('drawing.closeFirst')).catch(() => {});
          return;
        }
        router.push({ pathname: '/chat/[id]', params: { id: chat.id } });
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
        for (const note of localNotes(before, after)) jobs.track(showLocalNote(note));
      }
      before = after;
      jobs.track(
        updateBadge(
          after.conversations
            .filter((chat) => !chat.muted && !chat.archived)
            .reduce((total, chat) => total + chat.unread, 0),
        ),
      );
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
      if (!current() && !store.getSnapshot().erasePending)
        void jobs
          .drain()
          .then(() => clearDeviceNotifications(deviceId))
          .catch(() => {});
    };
  }, [store, deviceId, coordinator, jobs, dialogs]);
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
