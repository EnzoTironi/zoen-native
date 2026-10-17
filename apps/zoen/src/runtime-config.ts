import app from '../app.json';
import providers from '../config/identity-providers.json';
import { providerConfiguration } from './auth/configuration';

// Expo replaces these public values during bundling on both native and Web.
// Reading the same checked-in identifiers as CNG also works before a Web manifest exists.
export const identityProviders = providerConfiguration(
  {
    googleIosClientId: process.env.EXPO_PUBLIC_GOOGLE_IOS_CLIENT_ID ?? providers.googleIosClientId,
    googleWebClientId: process.env.EXPO_PUBLIC_GOOGLE_WEB_CLIENT_ID ?? providers.googleWebClientId,
    appleServiceId: process.env.EXPO_PUBLIC_APPLE_SERVICE_ID,
    appleRedirectUri: process.env.EXPO_PUBLIC_APPLE_REDIRECT_URI,
  },
  app.expo.ios.bundleIdentifier,
);

export const relayUrl = process.env.EXPO_PUBLIC_ZOEN_RELAY_URL || providers.relayUrl;
