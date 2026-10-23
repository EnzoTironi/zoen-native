import { pushConfiguration } from './configuration.ts';

export type IosPushBuild = {
  enabled: true;
  environment: 'sandbox' | 'production';
  topic: string;
};

/** Public build identifiers only. Never infer the APNs environment from JS dev mode. */
export function iosPushBuildConfiguration(
  enabled: boolean,
  configuration: unknown,
  bundleIdentifier: string | undefined,
  existingEntitlement: unknown,
): IosPushBuild | null {
  if (!enabled) return null;
  const parsed = pushConfiguration(configuration);
  if (!parsed || parsed.platform === 'fcm' || parsed.topic !== bundleIdentifier)
    throw new Error('iOS push requires an explicit APNs environment and the app bundle topic.');
  const entitlement = parsed.environment === 'sandbox' ? 'development' : 'production';
  if (existingEntitlement !== undefined && existingEntitlement !== entitlement)
    throw new Error('iOS push environment does not match the configured APNs entitlement.');
  return { enabled: true, environment: parsed.environment, topic: parsed.topic };
}

/** Preserve unrelated plugin options; one source owns the actual APNs/background switches. */
export function notificationPluginOptions(
  options: Record<string, unknown>,
  ios: IosPushBuild | null,
  channel: string | undefined,
) {
  return {
    ...options,
    ...(channel ? { defaultChannel: channel } : {}),
    enableRemoteNotifications: ios !== null,
    enableBackgroundRemoteNotifications: ios !== null,
  };
}
