import { t } from '../i18n/index.ts';
import { cancelled, challengeFromWire, serviceOrigin, verificationFromWire } from './guards.ts';
import type { AuthClient, AuthProvider, AuthService } from './types';

export type AuthAudiences = Partial<Record<AuthProvider, Partial<Record<AuthClient, string>>>>;

/** No cookies, provider secrets, retry replay, refresh tokens or persistent auth storage. */
export function createAuthService(
  baseUrl: string,
  audiences: AuthAudiences,
  fetcher: typeof fetch,
): AuthService {
  const origin = serviceOrigin(baseUrl);
  async function transport<T>(operation: () => Promise<T>, signal: AbortSignal): Promise<T> {
    try {
      return await operation();
    } catch (reason) {
      if (signal.aborted || cancelled(reason)) throw reason;
      throw new Error(t('authProviders.signInConnectionFailed'));
    }
  }
  async function post(path: string, input: unknown, signal: AbortSignal): Promise<unknown> {
    const response = await transport(
      () =>
        fetcher(`${origin}${path}`, {
          method: 'POST',
          redirect: 'error',
          credentials: 'omit',
          cache: 'no-store',
          signal,
          headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
          body: JSON.stringify(input),
        }),
      signal,
    );
    if (!response.ok) {
      if (response.status === 404 || response.status === 503)
        throw new Error(t('authProviders.thisSignInIsNotYetAvailableOn'));
      if (response.status === 429)
        throw new Error(t('authProviders.tooManyAttemptsWaitAMomentAndTry'));
      throw new Error(t('authProviders.theServerDidNotConfirmSignInTry'));
    }
    const reader = response.body?.getReader();
    if (!reader) throw new Error(t('authProviders.theServerReturnedAnIncompleteResponse'));
    const chunks: Uint8Array[] = [];
    let bytes = 0;
    try {
      for (;;) {
        const part = await transport(() => reader.read(), signal);
        if (part.done) break;
        bytes += part.value.byteLength;
        if (bytes > 32_768) throw new Error(t('authProviders.theServerReturnedAnInvalidResponse'));
        chunks.push(part.value);
      }
    } finally {
      await reader.cancel().catch(() => {});
      reader.releaseLock();
    }
    const merged = new Uint8Array(bytes);
    let offset = 0;
    for (const chunk of chunks) {
      merged.set(chunk, offset);
      offset += chunk.byteLength;
    }
    try {
      return JSON.parse(new TextDecoder().decode(merged));
    } catch {
      throw new Error(t('authProviders.theServerReturnedAnInvalidResponse'));
    }
  }
  return {
    async challenge(provider, client, signal) {
      const audience = audiences[provider]?.[client];
      if (!audience) throw new Error(t('authProviders.thisSignInIsNotConfiguredInThis'));
      return challengeFromWire(
        await post('/v1/auth/challenge', { provider, client }, signal),
        audience,
      );
    },
    async verify(challenge, credential, signal) {
      return verificationFromWire(
        await post(
          '/v1/auth/verify',
          { challenge_id: challenge.challenge_id, ...credential },
          signal,
        ),
      );
    },
  };
}
