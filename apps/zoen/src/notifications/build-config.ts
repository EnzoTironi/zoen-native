import { firebaseProjectId } from './configuration.ts';

const object = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

/** Public Firebase client config only. Service-account credentials are never an app config. */
export function androidFcmBuildConfiguration(
  enabled: boolean,
  services: unknown,
  packageName: string | undefined,
) {
  if (!enabled) return null;
  if (
    !object(services) ||
    !object(services.project_info) ||
    !firebaseProjectId(services.project_info.project_id) ||
    !packageName ||
    !Array.isArray(services.client) ||
    'private_key' in services
  )
    throw new Error('Android FCM requires google-services.json for this app package.');
  const matches = services.client.some(
    (client: unknown) =>
      object(client) &&
      object(client.client_info) &&
      object(client.client_info.android_client_info) &&
      client.client_info.android_client_info.package_name === packageName,
  );
  if (!matches) throw new Error('Android Firebase client package does not match Zoen.');
  return {
    enabled: true as const,
    platform: 'fcm' as const,
    projectId: services.project_info.project_id,
  };
}
