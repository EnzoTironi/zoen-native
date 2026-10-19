export type PushConfiguration =
  | { environment: 'sandbox' | 'production'; topic: string; platform?: 'apns' }
  | { platform: 'fcm'; projectId: string };

export const ANDROID_MESSAGE_CHANNEL = 'zoen-messages';

export function apnsToken(value: unknown): string | null {
  return typeof value === 'string' &&
    value.length >= 2 &&
    value.length <= 512 &&
    value.length % 2 === 0 &&
    /^[a-f0-9]+$/i.test(value)
    ? value.toLowerCase()
    : null;
}

/** Matches zoen-relay::push::valid_token; empty is reserved for explicit removal. */
export function fcmToken(value: unknown): string | null {
  return typeof value === 'string' &&
    value.length >= 1 &&
    value.length <= 4096 &&
    /^[A-Za-z0-9:_-]+$/.test(value)
    ? value
    : null;
}

export const firebaseProjectId = (value: unknown): value is string =>
  typeof value === 'string' && /^[a-z][a-z0-9-]{4,28}[a-z0-9]$/.test(value);

export function pushPlatform(configuration: PushConfiguration): 'apns' | 'apns-sandbox' | 'fcm' {
  return configuration.platform === 'fcm'
    ? 'fcm'
    : configuration.environment === 'sandbox'
      ? 'apns-sandbox'
      : 'apns';
}

export function pushToken(configuration: PushConfiguration, value: unknown): string | null {
  return configuration.platform === 'fcm' ? fcmToken(value) : apnsToken(value);
}

export function pushConfiguration(value: unknown): PushConfiguration | null {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return null;
  const input = value as Record<string, unknown>;
  if (input.platform === 'fcm')
    return firebaseProjectId(input.projectId)
      ? { platform: 'fcm', projectId: input.projectId }
      : null;
  if (
    (input.platform !== undefined && input.platform !== 'apns') ||
    !['sandbox', 'production'].includes(String(input.environment)) ||
    typeof input.topic !== 'string' ||
    input.topic.length > 255 ||
    !/^[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+$/.test(input.topic)
  )
    return null;
  return { environment: input.environment as 'sandbox' | 'production', topic: input.topic };
}
