import type { ConfigContext, ExpoConfig } from 'expo/config';
import providers from './config/identity-providers.json';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { androidFcmBuildConfiguration } from './src/notifications/build-config.ts';
import { ANDROID_MESSAGE_CHANNEL } from './src/notifications/configuration.ts';
import {
  iosPushBuildConfiguration,
  iosPushBuildEnvironment,
  notificationPluginOptions,
} from './src/notifications/ios-build-config.ts';

/** Public provider identifiers only. Provider secrets belong to the relay. */
export default ({ config, projectRoot }: ConfigContext): ExpoConfig => {
  const environmentIosPush = iosPushBuildEnvironment({
    enabled: process.env.ZOEN_IOS_PUSH_ENABLED,
    environment: process.env.ZOEN_IOS_PUSH_ENVIRONMENT,
    topic: process.env.ZOEN_IOS_PUSH_TOPIC,
  });
  const pushInput = environmentIosPush
    ? { ...config.extra?.zoenPush, ...environmentIosPush }
    : config.extra?.zoenPush;
  const iosPush = iosPushBuildConfiguration(
    pushInput?.enabled === true,
    pushInput,
    config.ios?.bundleIdentifier,
    config.ios?.entitlements?.['aps-environment'],
  );
  const androidFcmEnabled = process.env.ZOEN_ANDROID_FCM_ENABLED === 'true';
  const googleServicesFile =
    process.env.ZOEN_ANDROID_GOOGLE_SERVICES_FILE?.trim() || config.android?.googleServicesFile;
  if (androidFcmEnabled && !googleServicesFile)
    throw new Error('Android FCM requires a google-services.json file.');
  const androidPush = androidFcmBuildConfiguration(
    androidFcmEnabled,
    androidFcmEnabled
      ? JSON.parse(readFileSync(resolve(projectRoot, googleServicesFile!), 'utf8'))
      : null,
    config.android?.package,
  );
  const googleIosClientId =
    process.env.EXPO_PUBLIC_GOOGLE_IOS_CLIENT_ID?.trim() ?? providers.googleIosClientId;
  const googleWebClientId =
    process.env.EXPO_PUBLIC_GOOGLE_WEB_CLIENT_ID?.trim() ?? providers.googleWebClientId;
  const googleAndroidClientId = process.env.EXPO_PUBLIC_GOOGLE_ANDROID_CLIENT_ID?.trim();
  const appleServiceId = process.env.EXPO_PUBLIC_APPLE_SERVICE_ID?.trim();
  const appleRedirectUri = process.env.EXPO_PUBLIC_APPLE_REDIRECT_URI?.trim();
  const plugins = [
    ...(config.plugins ?? []),
    './plugins/with-native-surfaces.cjs',
    './plugins/with-native-calls.cjs',
    './plugins/with-native-window-policy.cjs',
    './plugins/with-shortcut-localizations.cjs',
    './plugins/with-android-back-dispatch.cjs',
  ];
  if (googleIosClientId) {
    const suffix = '.apps.googleusercontent.com';
    if (!googleIosClientId.endsWith(suffix)) throw new Error('Invalid Google iOS client ID.');
    plugins.push([
      'react-native-nitro-google-signin',
      { iosUrlScheme: `com.googleusercontent.apps.${googleIosClientId.slice(0, -suffix.length)}` },
    ]);
  }
  const notificationPlugins = plugins.filter(
    (plugin) => (Array.isArray(plugin) ? plugin[0] : plugin) === 'expo-notifications',
  );
  if (notificationPlugins.length > 1) throw new Error('Duplicate notifications build plugin.');
  if (iosPush || androidPush || notificationPlugins.length) {
    const existing = notificationPlugins[0];
    const options = Array.isArray(existing) ? existing[1] : {};
    if (
      options !== undefined &&
      (options === null || typeof options !== 'object' || Array.isArray(options))
    )
      throw new Error('Invalid notifications build plugin options.');
    const configured = [
      'expo-notifications',
      notificationPluginOptions(
        options ?? {},
        iosPush,
        androidPush ? ANDROID_MESSAGE_CHANNEL : undefined,
      ),
    ] as [string, Record<string, unknown>];
    if (existing) plugins.splice(plugins.indexOf(existing), 1, configured);
    else plugins.push(configured);
  }
  return {
    ...config,
    name: config.name ?? 'Zoen',
    slug: config.slug ?? 'zoen',
    plugins,
    ios: {
      ...config.ios,
      ...(iosPush
        ? {
            entitlements: {
              ...config.ios?.entitlements,
              'aps-environment': iosPush.environment === 'sandbox' ? 'development' : 'production',
            },
          }
        : {}),
      infoPlist: {
        ...config.ios?.infoPlist,
        ZoenRemoteNotificationsEnabled: iosPush !== null,
        ZoenPushEnvironment: iosPush?.environment ?? '',
        ZoenPushTopic: iosPush?.topic ?? '',
      },
    },
    ...(androidPush ? { android: { ...config.android, googleServicesFile } } : {}),
    extra: {
      ...config.extra,
      ...(environmentIosPush ? { zoenPush: pushInput } : {}),
      ...(androidPush ? { zoenPush: { ...pushInput, android: androidPush } } : {}),
      zoenRelayUrl: process.env.EXPO_PUBLIC_ZOEN_RELAY_URL?.trim() || providers.relayUrl,
      zoenAuth: {
        googleIosClientId,
        googleAndroidClientId,
        googleWebClientId,
        appleServiceId,
        appleRedirectUri,
      },
    },
  };
};
