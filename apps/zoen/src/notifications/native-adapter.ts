import { pushConfiguration, type PushConfiguration } from './configuration.ts';
import { decodeWake, type PushWake } from './wake.ts';

const object = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

export const nativeNotificationPlatform = (platform: string): platform is 'ios' | 'android' =>
  platform === 'ios' || platform === 'android';

/** Android has an independent explicit build opt-in; APNs settings never enable FCM. */
export function nativePushConfigurationFor(
  platform: string,
  extra: unknown,
): PushConfiguration | null {
  if (!nativeNotificationPlatform(platform) || !object(extra)) return null;
  const value = platform === 'android' ? extra.android : extra;
  if (!object(value) || value.enabled !== true) return null;
  const parsed = pushConfiguration(value);
  return parsed && (platform === 'android' ? parsed.platform === 'fcm' : parsed.platform !== 'fcm')
    ? parsed
    : null;
}

/** Do not prompt or create a channel from a passive foreground permission read. */
export async function readNativePermission<T>(
  platform: string,
  request: boolean,
  port: { channel(): Promise<unknown>; read(): Promise<T>; prompt(): Promise<T> },
): Promise<T> {
  if (request && platform === 'android') await port.channel();
  return request ? port.prompt() : port.read();
}

export function nativeTokenFromWire(platform: string, value: unknown): string | null {
  return nativeNotificationPlatform(platform) &&
    object(value) &&
    value.type === platform &&
    typeof value.data === 'string'
    ? value.data
    : null;
}

export async function readNativeToken(
  platform: string,
  port: { channel(): Promise<unknown>; token(): Promise<unknown> },
): Promise<string> {
  if (!nativeNotificationPlatform(platform)) throw new Error('unsupported push platform');
  if (platform === 'android') await port.channel();
  const token = nativeTokenFromWire(platform, await port.token());
  if (token === null) throw new Error('unsupported push token');
  return token;
}

/** Expo58 exposes APNs in payload and Firebase in remoteMessage, not content.data. */
export function decodeNotificationWake(platform: string, trigger: unknown): PushWake | null {
  if (!object(trigger) || trigger.type !== 'push') return null;
  if (platform === 'ios') return decodeWake(trigger.payload, 'apns');
  if (platform !== 'android' || !object(trigger.remoteMessage)) return null;
  const message = trigger.remoteMessage;
  if (message.notification !== null || !object(message.data)) return null;
  const data = message.data;
  const keys = Object.keys(data).sort().join(',');
  // Expo's Android RemoteMessageSerializer adds dataString:null for absent body.
  if (keys !== 'wake' && !(keys === 'dataString,wake' && data.dataString === null)) return null;
  return decodeWake({ wake: data.wake }, 'fcm');
}
