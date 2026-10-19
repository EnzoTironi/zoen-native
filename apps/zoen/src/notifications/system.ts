import Constants from 'expo-constants';
import * as Notifications from 'expo-notifications';
import { Platform } from 'react-native';
import type { PushPermission } from './coordinator';
import { ANDROID_MESSAGE_CHANNEL } from './configuration';
import {
  nativePushConfigurationFor,
  readNativePermission,
  readNativeToken,
  nativeTokenFromWire,
  decodeNotificationWake,
} from './native-adapter';
import type { PushWake } from './wake';
import { t } from '../i18n';
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
  return nativePushConfigurationFor(Platform.OS, Constants.expoConfig?.extra?.zoenPush);
}

async function messageChannel() {
  return Notifications.setNotificationChannelAsync(ANDROID_MESSAGE_CHANNEL, {
    name: t('notifications.notifications'),
    importance: Notifications.AndroidImportance.DEFAULT,
    sound: 'default',
    lockscreenVisibility: Notifications.AndroidNotificationVisibility.PRIVATE,
  });
}

export async function notificationPermission(
  request: boolean,
): Promise<Exclude<PushPermission, 'unknown'>> {
  const permission = await readNativePermission(Platform.OS, request, {
    channel: messageChannel,
    read: Notifications.getPermissionsAsync,
    prompt: () =>
      Notifications.requestPermissionsAsync({
        ios: { allowAlert: true, allowSound: true, allowBadge: true },
      }),
  });
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
  return readNativeToken(Platform.OS, {
    channel: messageChannel,
    token: Notifications.getDevicePushTokenAsync,
  });
}

export function listenPushToken(listener: (token: unknown) => void) {
  const subscription = Notifications.addPushTokenListener((token) => {
    const value = nativeTokenFromWire(Platform.OS, token);
    if (value !== null) listener(value);
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
  wake: (payload: PushWake) => void,
  tap: (data: unknown) => void,
) {
  let active = true;
  const received = Notifications.addNotificationReceivedListener((notification) => {
    const trigger = notification.request.trigger;
    if (!active) return;
    const hint = decodeNotificationWake(Platform.OS, trigger);
    if (hint) wake(hint);
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
  if (Platform.OS === 'android') await messageChannel();
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
    trigger: Platform.OS === 'android' ? { channelId: ANDROID_MESSAGE_CHANNEL } : null,
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
