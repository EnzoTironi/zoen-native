import Constants from 'expo-constants';
import * as Notifications from 'expo-notifications';
import { Platform } from 'react-native';
import { pushConfiguration, type PushPermission } from './coordinator';
import { clearDeviceNotificationState } from './clear';
import type { LocalNote } from './local';

function isPushTrigger(
  trigger: Notifications.Notification['request']['trigger'],
): trigger is Notifications.PushNotificationTrigger {
  return (
    typeof trigger === 'object' && trigger !== null && 'type' in trigger && trigger.type === 'push'
  );
}

export function nativePushConfiguration() {
  if (Platform.OS !== 'ios') return null;
  const config: unknown = Constants.expoConfig?.extra?.zoenPush;
  if (
    typeof config !== 'object' ||
    config === null ||
    !('enabled' in config) ||
    config.enabled !== true
  )
    return null;
  return pushConfiguration(config);
}

export async function notificationPermission(
  request: boolean,
): Promise<Exclude<PushPermission, 'unknown'>> {
  const permission = request
    ? await Notifications.requestPermissionsAsync({
        ios: { allowAlert: true, allowSound: true, allowBadge: true },
      })
    : await Notifications.getPermissionsAsync();
  if (
    permission.granted ||
    (permission.ios &&
      [
        Notifications.IosAuthorizationStatus.AUTHORIZED,
        Notifications.IosAuthorizationStatus.PROVISIONAL,
        Notifications.IosAuthorizationStatus.EPHEMERAL,
      ].includes(permission.ios.status))
  )
    return 'allowed';
  return permission.status === 'undetermined' ? 'undetermined' : 'denied';
}

export async function nativePushToken() {
  const token = await Notifications.getDevicePushTokenAsync();
  if (token.type !== 'ios' || typeof token.data !== 'string')
    throw new Error('unsupported push token');
  return token.data;
}

export function listenPushToken(listener: (token: unknown) => void) {
  const subscription = Notifications.addPushTokenListener((token) => {
    if (token.type === 'ios') listener(token.data);
  });
  return () => subscription.remove();
}

export function installNotificationHandler(allowLocal: (data: unknown) => boolean) {
  Notifications.setNotificationHandler({
    handleNotification: async (notification) => {
      const local =
        !isPushTrigger(notification.request.trigger) &&
        allowLocal(notification.request.content.data);
      return {
        shouldPlaySound: Boolean(local),
        shouldSetBadge: false,
        shouldShowBanner: Boolean(local),
        shouldShowList: Boolean(local),
      };
    },
  });
  return () => Notifications.setNotificationHandler(null);
}

export function listenNotificationEvents(
  wake: (payload: unknown) => void,
  tap: (data: unknown) => void,
) {
  let active = true;
  const received = Notifications.addNotificationReceivedListener((notification) => {
    const trigger = notification.request.trigger;
    if (active && isPushTrigger(trigger)) wake(trigger.payload);
  });
  const respond = (response: Notifications.NotificationResponse | null) => {
    if (
      !active ||
      !response ||
      response.actionIdentifier !== Notifications.DEFAULT_ACTION_IDENTIFIER ||
      isPushTrigger(response.notification.request.trigger)
    )
      return;
    tap(response.notification.request.content.data);
    void Notifications.clearLastNotificationResponseAsync();
  };
  const responses = Notifications.addNotificationResponseReceivedListener(respond);
  void Notifications.getLastNotificationResponseAsync()
    .then(respond)
    .catch(() => {});
  return () => {
    active = false;
    received.remove();
    responses.remove();
  };
}

export async function showLocalNote(note: LocalNote) {
  await Notifications.scheduleNotificationAsync({
    identifier: note.identifier,
    content: {
      title: note.title,
      body: note.body,
      sound: 'default',
      threadIdentifier: note.spaceId,
      data: {
        zoen_local: 1,
        device_id: note.deviceId,
        space_id: note.spaceId,
        event_seq: note.sequence,
      },
    },
    trigger: null,
  });
}

export async function updateBadge(count: number) {
  await Notifications.setBadgeCountAsync(Math.max(0, Math.min(999999, count)));
}

export async function clearDeviceNotifications(deviceId: string) {
  await clearDeviceNotificationState(deviceId, {
    presented: async () =>
      (await Notifications.getPresentedNotificationsAsync()).map((note) => note.request),
    scheduled: Notifications.getAllScheduledNotificationsAsync,
    dismiss: Notifications.dismissNotificationAsync,
    cancel: Notifications.cancelScheduledNotificationAsync,
    clearBadge: () => Notifications.setBadgeCountAsync(0),
    clearResponse: Notifications.clearLastNotificationResponseAsync,
  });
}
