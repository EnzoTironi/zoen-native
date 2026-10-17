import { t } from '../i18n/index.ts';
import type { VerifiedSignIn } from './types';

export type RecoveryAccount = { identityId: string; handle: string };

/** A recognized account goes to key recovery before any replacement keys are made. */
export async function completeProviderSession(
  result: VerifiedSignIn,
  localIdentity: string | undefined,
  provision: (proof: string, name: string) => Promise<unknown>,
  recover: (account: RecoveryAccount) => void,
) {
  if (result.expires_at_ms <= Date.now())
    throw new Error(t('authProviders.confirmYourSignInAgainToContinue'));
  if (result.account.kind === 'bound' && result.account.identity_id !== localIdentity) {
    recover({ identityId: result.account.identity_id, handle: result.account.handle });
    return;
  }
  await provision(
    result.proof_token,
    result.profile?.name ?? result.suggested_name ?? t('authProviders.zoenPerson'),
  );
}
