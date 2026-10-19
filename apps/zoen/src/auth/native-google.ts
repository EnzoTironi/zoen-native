import type * as GoogleSignIn from 'react-native-nitro-google-signin';
import { t } from '../i18n/index.ts';
import { AuthCancelled, profileName } from './guards.ts';
import type { AuthTicket } from './attempt';
import type { AuthClient, ProviderConfiguration, ProviderCredential } from './types';

export type NativeAuthClient = Extract<AuthClient, 'ios' | 'android'>;

export function nativeAuthClient(platform: string): NativeAuthClient | null {
  return platform === 'ios' || platform === 'android' ? platform : null;
}

/** A server audience never substitutes for the OAuth registration of this signed app. */
export function nativeGoogleConfigured(
  configuration: ProviderConfiguration,
  client: NativeAuthClient,
): boolean {
  const google = configuration.google;
  return Boolean(
    google?.webClientId && (client === 'ios' ? google.iosClientId : google.androidClientId),
  );
}

export type GoogleCredentialSdk = Pick<
  typeof GoogleSignIn,
  'isErrorWithCode' | 'isCancelledResponse' | 'isSuccessResponse'
> & {
  statusCodes: Pick<typeof GoogleSignIn.statusCodes, 'SIGN_IN_CANCELLED'>;
  GoogleOneTapSignIn: Pick<
    typeof GoogleSignIn.GoogleOneTapSignIn,
    'configure' | 'presentExplicitSignIn' | 'signOut'
  >;
};

/** Both mobile SDKs request a fresh nonce-bound ID token for the server's Web audience. */
export async function nativeGoogleCredential(
  ticket: AuthTicket,
  google: GoogleCredentialSdk,
  configuration: ProviderConfiguration,
): Promise<{ credential: ProviderCredential; suggestedName?: string }> {
  const client = nativeAuthClient(ticket.client);
  if (ticket.provider !== 'google' || !client || !nativeGoogleConfigured(configuration, client))
    throw new Error(t('authProviders.thisSignInIsNotAvailableInThe'));
  if (ticket.signal.aborted) throw new AuthCancelled();
  google.GoogleOneTapSignIn.configure({
    webClientId: configuration.google!.webClientId,
    ...(client === 'ios' ? { iosClientId: configuration.google!.iosClientId } : {}),
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
    // Google credentials are not a second persistent Zoen session.
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
