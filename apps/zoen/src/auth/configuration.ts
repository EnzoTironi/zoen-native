import type { AuthAudiences } from './client';
import type { ProviderConfiguration } from './types';

const googleId = (value: unknown): string | undefined =>
  typeof value === 'string' && /^\d+-[a-z0-9]+\.apps\.googleusercontent\.com$/.test(value.trim())
    ? value.trim()
    : undefined;
const appleId = (value: unknown): string | undefined =>
  typeof value === 'string' && /^[a-zA-Z0-9.-]{3,200}$/.test(value.trim())
    ? value.trim()
    : undefined;

/** Public configuration only; missing values remain missing rather than demo credentials. */
export function providerConfiguration(
  extra: unknown,
  bundleIdentifier: string | undefined,
): ProviderConfiguration {
  const values = extra && typeof extra === 'object' ? (extra as Record<string, unknown>) : {};
  const webClientId = googleId(values.googleWebClientId);
  const iosClientId = googleId(values.googleIosClientId);
  const androidClientId = googleId(values.googleAndroidClientId);
  const iosAudience = appleId(bundleIdentifier);
  return {
    ...(webClientId ? { google: { webClientId, iosClientId, androidClientId } } : {}),
    ...(iosAudience
      ? {
          apple: {
            iosAudience,
            serviceId: appleId(values.appleServiceId),
            redirectUri:
              typeof values.appleRedirectUri === 'string' ? values.appleRedirectUri : undefined,
          },
        }
      : {}),
  };
}

export function authAudiences(configuration: ProviderConfiguration): AuthAudiences {
  return {
    google: {
      ios: configuration.google?.webClientId,
      android: configuration.google?.androidClientId ? configuration.google.webClientId : undefined,
      web: configuration.google?.webClientId,
    },
    apple: { ios: configuration.apple?.iosAudience, web: configuration.apple?.serviceId },
  };
}
