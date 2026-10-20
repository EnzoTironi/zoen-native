import type { ConfigContext, ExpoConfig } from 'expo/config';
import providers from './config/identity-providers.json';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { androidFcmBuildConfiguration } from './src/notifications/build-config.ts';
import { ANDROID_MESSAGE_CHANNEL } from './src/notifications/configuration.ts';

/** Public provider identifiers only. Provider secrets belong to the relay. */
export default ({ config, projectRoot }: ConfigContext): ExpoConfig => {
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
  if (
    androidPush &&
    !plugins.some((plugin) => (Array.isArray(plugin) ? plugin[0] : plugin) === 'expo-notifications')
  ) {
    plugins.push([
      'expo-notifications',
      {
        defaultChannel: ANDROID_MESSAGE_CHANNEL,
        enableRemoteNotifications: config.extra?.zoenPush?.enabled === true,
        enableBackgroundRemoteNotifications: false,
      },
    ]);
  }
  return {
    ...config,
    name: config.name ?? 'Zoen',
    slug: config.slug ?? 'zoen',
    plugins,
    ...(androidPush ? { android: { ...config.android, googleServicesFile } } : {}),
    extra: {
      ...config.extra,
      ...(androidPush ? { zoenPush: { ...config.extra?.zoenPush, android: androidPush } } : {}),
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
