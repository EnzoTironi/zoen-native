import type { ConfigContext, ExpoConfig } from 'expo/config';
import providers from './config/identity-providers.json';

/** Public OAuth identifiers only. Provider secrets belong to the relay. */
export default ({ config }: ConfigContext): ExpoConfig => {
  const googleIosClientId =
    process.env.EXPO_PUBLIC_GOOGLE_IOS_CLIENT_ID?.trim() ?? providers.googleIosClientId;
  const googleWebClientId =
    process.env.EXPO_PUBLIC_GOOGLE_WEB_CLIENT_ID?.trim() ?? providers.googleWebClientId;
  const appleServiceId = process.env.EXPO_PUBLIC_APPLE_SERVICE_ID?.trim();
  const appleRedirectUri = process.env.EXPO_PUBLIC_APPLE_REDIRECT_URI?.trim();
  const plugins = [
    ...(config.plugins ?? []),
    './plugins/with-native-surfaces.cjs',
    './plugins/with-shortcut-localizations.cjs',
  ];
  if (googleIosClientId) {
    const suffix = '.apps.googleusercontent.com';
    if (!googleIosClientId.endsWith(suffix)) throw new Error('Invalid Google iOS client ID.');
    plugins.push([
      'react-native-nitro-google-signin',
      { iosUrlScheme: `com.googleusercontent.apps.${googleIosClientId.slice(0, -suffix.length)}` },
    ]);
  }
  return {
    ...config,
    name: config.name ?? 'Zoen',
    slug: config.slug ?? 'zoen',
    plugins,
    extra: {
      ...config.extra,
      zoenRelayUrl: process.env.EXPO_PUBLIC_ZOEN_RELAY_URL?.trim() || providers.relayUrl,
      zoenAuth: {
        googleIosClientId,
        googleWebClientId,
        appleServiceId,
        appleRedirectUri,
      },
    },
  };
};
