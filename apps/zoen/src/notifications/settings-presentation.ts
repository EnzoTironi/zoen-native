import type { notifications } from '../i18n/catalogs/notifications.ts';
import type { PushState } from './coordinator.ts';

type Copy = typeof notifications.en;
type NotificationCopy = {
  [Key in keyof Copy]: Copy[Key] extends `${string}%{${string}` ? never : Key;
}[keyof Copy];
type NotificationAction = 'request' | 'refresh' | 'settings';

/** Permission and registration are separate facts; neither verifies delivery. */
export function notificationSettingsPresentation(state: PushState, native: boolean) {
  const busy = state.busy || state.registration === 'registering';
  const view = (
    title: NotificationCopy,
    detail: NotificationCopy | null,
    action: NotificationAction | null,
    label: NotificationCopy,
  ) => ({
    title,
    detail,
    action,
    label: (busy ? 'settingUp' : label) as NotificationCopy,
    disabled: busy,
  });
  if (!native)
    return view(
      'unavailableInThisBrowser',
      'notificationsAreNotYetAvailableInThisBrowser',
      null,
      'checkAgain',
    );
  if (state.permission === 'unknown') return view('checking', null, 'refresh', 'checkAgain');
  if (state.permission === 'denied')
    return view('disabledInSettings', null, 'settings', 'openSettings');
  if (state.permission === 'undetermined')
    return view('notificationsOff', null, 'request', 'allowNotifications');
  if (state.registration === 'unsupported')
    return view(
      'allowedOnDevice',
      'remoteNotificationsAreNotYetAvailableOnThis',
      'settings',
      'openSettings',
    );
  if (state.failure) return view('setupPending', 'permissionGranted', 'refresh', 'tryAgain');
  if (state.registration === 'acknowledged')
    return view('allowedOnDevice', null, 'settings', 'openSettings');
  if (state.registration === 'waiting' || state.registration === 'refused')
    return view('setupPending', 'permissionGranted', 'refresh', 'tryAgain');
  return view('setupPending', 'permissionGranted', 'refresh', 'finishSetup');
}

/** Invoked only from an explicit user action; refresh never prompts for permission. */
export function performNotificationAction(
  action: NotificationAction | null,
  ports: {
    requestPermission(): Promise<void>;
    refresh(): Promise<void>;
    openSettings(): Promise<unknown>;
  },
) {
  if (action === 'request') return ports.requestPermission();
  if (action === 'refresh') return ports.refresh();
  if (action === 'settings') return ports.openSettings();
  return Promise.resolve();
}
