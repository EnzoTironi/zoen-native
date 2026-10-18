import { t } from '../i18n/index.ts';
import { requireOptionalNativeModule } from 'expo';
import { Platform, TurboModuleRegistry } from 'react-native';
import type * as AppleAuthentication from 'expo-apple-authentication';
import type * as GoogleSignIn from 'react-native-nitro-google-signin';
import { AuthCancelled, profileName } from './guards';
import type { AuthTicket } from './attempt';
import type { ProviderConfiguration, ProviderCredential } from './types';

export type NativeProviders = {
  apple?: typeof AppleAuthentication;
  google?: typeof GoogleSignIn;
};
let presenting = false;

export async function loadNativeProviders(
  configuration: ProviderConfiguration,
): Promise<NativeProviders> {
  if (Platform.OS !== 'ios') return {};
  const modules: NativeProviders = {};
  if (configuration.apple && requireOptionalNativeModule('ExpoAppleAuthentication')) {
    const apple = await import('expo-apple-authentication');
    if (await apple.isAvailableAsync().catch(() => false)) modules.apple = apple;
  }
  // Do not evaluate Nitro's package until its TurboModule exists in this binary.
  if (configuration.google?.iosClientId && TurboModuleRegistry.get('NitroModules')) {
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
    if (ticket.provider === 'apple' && modules.apple) {
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
    if (ticket.provider === 'google' && modules.google && configuration.google?.iosClientId) {
      const google = modules.google;
      google.GoogleOneTapSignIn.configure({
        webClientId: configuration.google.webClientId,
        iosClientId: configuration.google.iosClientId,
        nonce: ticket.challenge.nonce,
        scopes: [],
        offlineAccess: false,
        autoSelectOnSignIn: false,
      });
      let response: Awaited<ReturnType<typeof google.GoogleOneTapSignIn.presentExplicitSignIn>>;
      try {
        response = await google.GoogleOneTapSignIn.presentExplicitSignIn();
      } catch (reason) {
        if (google.isErrorWithCode(reason) && reason.code === google.statusCodes.SIGN_IN_CANCELLED)
          throw new AuthCancelled();
        throw new Error(t('authProviders.couldNotContinueWithGoogleTryAgain'));
      } finally {
        // The ID token goes only to our verifier. Clear the SDK's app session rather
        // than retaining Google refresh/access credentials as a second login store.
        await google.GoogleOneTapSignIn.signOut().catch(() => {});
      }
      if (ticket.signal.aborted || google.isCancelledResponse(response)) throw new AuthCancelled();
      if (!google.isSuccessResponse(response) || !response.data.idToken)
        throw new Error(t('authProviders.googleDidNotConfirmSignInTryAgain'));
      return {
        credential: { id_token: response.data.idToken },
        suggestedName: profileName(response.data.user.name),
      };
    }
    throw new Error(t('authProviders.thisSignInIsNotAvailableInThe'));
  } finally {
    presenting = false;
  }
}
