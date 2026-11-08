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
import { accountScope, accountScopeKey } from '../core/account-scope';
import { useDialogs } from '../components/dialogs';
import { PushCoordinator } from './coordinator';
import { validLocalNotification } from './links';
import { allowLocalNotification } from './local-admission';
import { nativeNotificationPlatform } from './native-adapter';
import { pushPlatform } from './configuration';
import { backgroundPresentationJobs } from './presentation';
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
  nativeNotificationPresentation,
  requestNativeNotes,
  updateBadge,
} from './system';

const Context = createContext<PushCoordinator | null>(null);

export function NotificationProvider({ children }: { children: ReactNode }) {
  const client = useClient();
  const { store, snapshot, mode } = client;
  const dialogs = useDialogs();
  const sessionEpoch = store.sessionEpoch;
  const deviceId = mode === 'account' ? (snapshot?.account?.device_id ?? null) : null;
  const jobs = useMemo(() => (deviceId ? new NotificationJobs() : null), [deviceId]);
  const identityId = mode === 'account' ? (snapshot?.account?.identity_id ?? null) : null;
  const relay = mode === 'account' ? snapshot?.account?.relay_url : undefined;
  const scope = accountScopeKey(accountScope(store, client));
  const coordinator = useMemo(() => {
    const current = () => {
      return !!scope && accountScopeKey(accountScope(store)) === scope;
    };
    return new PushCoordinator(
      {
        current,
        permission: notificationPermission,
        token: nativePushToken,
        async register(configuration, token) {
          if (!identityId || !deviceId || !current())
            throw new Error(t('notifications.accountChanged'));
          await store.driver.request({
            op: 'register_push',
            expected_identity_id: identityId,
            expected_device_id: deviceId,
            platform: pushPlatform(configuration),
            token,
          });
          return { kind: 'acknowledged' };
        },
      },
      nativePushConfiguration(),
    );
  }, [store, identityId, deviceId, scope]);

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
    const sameDevice = () =>
      store.getSnapshot().mode === 'account' &&
      store.getSnapshot().snapshot?.account?.device_id === deviceId;
    const current = () => sameDevice() && store.sessionEpoch === sessionEpoch;
    const canOpen = (data: unknown) => {
      const state = store.getSnapshot();
      const owner = accountScope(store, state);
      if (
        !owner ||
        owner.identity !== identityId ||
        owner.device !== deviceId ||
        owner.relay !== relay ||
        owner.epoch !== sessionEpoch
      )
        return null;
      const target = validLocalNotification(data, deviceId);
      const chat =
        target &&
        state.snapshot?.conversations.find(
          (chat) =>
            chat.id === target.spaceId &&
            chat.members.some((member) => member.id === owner.identity),
        );
      return chat && target ? { chat, target } : null;
    };
    const clearHandler = installNotificationHandler(async (data) =>
      current() ? allowLocalNotification(store, data, deviceId) : false,
    );
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
        const destination = canOpen(data);
        if (!destination) return;
        try {
          store.assertExternalNavigationAllowed();
        } catch {
          void dialogs.inform(t('drawing.pending'), t('drawing.closeFirst')).catch(() => {});
          return;
        }
        const { chat, target } = destination;
        router.push(
          target.messageId
            ? {
                pathname: '/message/[space]/[message]',
                params: { space: chat.id, message: target.messageId },
              }
            : { pathname: '/chat/[id]', params: { id: chat.id } },
        );
      },
    );
    let before = store.getSnapshot().snapshot;
    let wasSynced = before?.connection.synced === true;
    const stopStore = store.subscribe(() => {
      const after = store.getSnapshot().snapshot;
      if (!current()) return;
      const synced = after?.connection.synced === true;
      const reconnected = synced && !wasSynced;
      wasSynced = synced;
      if (!synced) return;
      if (
        reconnected &&
        AppState.currentState === 'active' &&
        coordinator.getSnapshot().registration === 'waiting'
      ) {
        void coordinator.foreground();
      }
      if (
        before &&
        AppState.currentState !== 'active' &&
        coordinator.getSnapshot().permission === 'allowed'
      ) {
        for (const task of backgroundPresentationJobs(
          before,
          after,
          nativeNotificationPresentation(),
          {
            native: requestNativeNotes,
            local: showLocalNote,
          },
        ))
          jobs.track(task);
      }
      before = after;
      jobs.track(
        updateBadge(
          after.conversations
            .filter((chat) => !chat.muted && !chat.archived)
            .reduce((total, chat) => total + Math.max(chat.unread, chat.manual_unread ? 1 : 0), 0),
        ),
      );
    });
    const foreground = AppState.addEventListener('change', (state) => {
      if (state === 'active') void coordinator.foreground();
    });
    // A background-launched React tree must not start token registration work.
    if (AppState.currentState === 'active') void coordinator.foreground();
    return () => {
      stopStore();
      stopEvents();
      stopToken();
      clearHandler();
      foreground.remove();
      coordinator.dispose();
      if (!sameDevice() && !store.getSnapshot().erasePending)
        void jobs
          .drain()
          .then(() => clearDeviceNotifications(deviceId))
          .catch(() => {});
    };
  }, [store, deviceId, coordinator, jobs, dialogs, identityId, relay, sessionEpoch]);
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
