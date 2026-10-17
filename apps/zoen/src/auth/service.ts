import { fetch as expoFetch } from 'expo/fetch';
import { createAuthService } from './client';
import { authAudiences } from './configuration';
import type { ProviderConfiguration } from './types';

export function createExpoAuthService(relayUrl: string, configuration: ProviderConfiguration) {
  return createAuthService(relayUrl, authAudiences(configuration), expoFetch as typeof fetch);
}
