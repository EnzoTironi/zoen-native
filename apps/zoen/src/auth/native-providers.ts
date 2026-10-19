import { t } from '../i18n/index.ts';
import { requireOptionalNativeModule } from 'expo';
import { Platform, TurboModuleRegistry } from 'react-native';
import type * as AppleAuthentication from 'expo-apple-authentication';
import type * as GoogleSignIn from 'react-native-nitro-google-signin';
import { AuthCancelled, profileName } from './guards';
import type { AuthTicket } from './attempt';
import type { ProviderConfiguration, ProviderCredential } from './types';
import { nativeAuthClient, nativeGoogleConfigured, nativeGoogleCredential } from './native-google';

export type NativeProviders = {
  apple?: typeof AppleAuthentication;
  google?: typeof GoogleSignIn;
};
let presenting = false;

export async function loadNativeProviders(
  configuration: ProviderConfiguration,
): Promise<NativeProviders> {
  const client = nativeAuthClient(Platform.OS);
  if (!client) return {};
  const modules: NativeProviders = {};
  if (
    client === 'ios' &&
    configuration.apple &&
    requireOptionalNativeModule('ExpoAppleAuthentication')
  ) {
    const apple = await import('expo-apple-authentication');
    if (await apple.isAvailableAsync().catch(() => false)) modules.apple = apple;
  }
  // Do not evaluate Nitro's package until its TurboModule exists in this binary.
  if (nativeGoogleConfigured(configuration, client) && TurboModuleRegistry.get('NitroModules')) {
    try {
      modules.google = await import('react-native-nitro-google-signin');
    } catch {
      /* A binary predating the Google hybrid object remains usable. */
    }
  }
  return modules;
}

export async function nativeCredential(
  ticket: AuthTicket,
  modules: NativeProviders,
  configuration: ProviderConfiguration,
): Promise<{ credential: ProviderCredential; suggestedName?: string }> {
  if (presenting) throw new Error(t('authProviders.finishTheOpenSignInBeforeStartingAnother'));
  if (ticket.signal.aborted) throw new AuthCancelled();
  presenting = true;
  try {
    if (ticket.provider === 'apple' && ticket.client === 'ios' && modules.apple) {
      const apple = modules.apple;
      let response: Awaited<ReturnType<typeof apple.signInAsync>>;
      try {
        response = await apple.signInAsync({
          nonce: ticket.challenge.nonce,
          state: ticket.challenge.state,
          requestedScopes: [
            apple.AppleAuthenticationScope.FULL_NAME,
            apple.AppleAuthenticationScope.EMAIL,
          ],
        });
      } catch (reason) {
        if (
          reason &&
          typeof reason === 'object' &&
          'code' in reason &&
          reason.code === 'ERR_REQUEST_CANCELED'
        )
          throw new AuthCancelled();
        throw new Error(t('authProviders.couldNotContinueWithAppleTryAgain'));
      }
      if (ticket.signal.aborted) throw new AuthCancelled();
      if (!response.identityToken || !response.authorizationCode)
        throw new Error(t('authProviders.appleDidNotConfirmSignInTryAgain'));
      const suggestedName = response.fullName
        ? profileName(apple.formatFullName(response.fullName))
        : undefined;
      return {
        credential: {
          id_token: response.identityToken,
          authorization_code: response.authorizationCode,
          state: response.state ?? undefined,
        },
        suggestedName,
      };
    }
    if (ticket.provider === 'google' && modules.google)
      return await nativeGoogleCredential(ticket, modules.google, configuration);
    throw new Error(t('authProviders.thisSignInIsNotAvailableInThe'));
  } finally {
    presenting = false;
  }
}
