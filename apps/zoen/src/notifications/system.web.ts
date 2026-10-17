import { t } from '../i18n/index.ts';
import type { LocalNote } from './local';

export const nativePushConfiguration = () => null;
export const notificationPermission = async (_request: boolean) => 'denied' as const;
export const nativePushToken = async (): Promise<string> => {
  throw new Error(t('notifications.remoteBrowserNotificationsAreNotSetUpYet'));
};
export const listenPushToken = (_listener: (token: unknown) => void) => () => {};
export const installNotificationHandler = (_allowLocal: (data: unknown) => boolean) => () => {};
export const listenNotificationEvents =
  (_wake: (payload: unknown) => void, _tap: (data: unknown) => void) => () => {};
export const showLocalNote = async (_note: LocalNote) => {};
export const updateBadge = async (_count: number) => {};
export const clearDeviceNotifications = async (_deviceId: string) => {};
