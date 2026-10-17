import { challengeFromWire, serviceOrigin, verificationFromWire } from './guards.ts';
import type { AuthClient, AuthProvider, AuthService } from './types';

export type AuthAudiences = Partial<Record<AuthProvider, Partial<Record<AuthClient, string>>>>;

/** No cookies, provider secrets, retry replay, refresh tokens or persistent auth storage. */
export function createAuthService(
  baseUrl: string,
  audiences: AuthAudiences,
  fetcher: typeof fetch,
): AuthService {
  const origin = serviceOrigin(baseUrl);
  async function post(path: string, input: unknown, signal: AbortSignal): Promise<unknown> {
    const response = await fetcher(`${origin}${path}`, {
      method: 'POST',
      redirect: 'error',
      credentials: 'omit',
      cache: 'no-store',
      signal,
      headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
      body: JSON.stringify(input),
    });
    if (!response.ok) {
      if (response.status === 404 || response.status === 503)
        throw new Error('Este login ainda não está disponível no servidor.');
      if (response.status === 429)
        throw new Error('Muitas tentativas. Aguarde um pouco e tente novamente.');
      throw new Error('O servidor não confirmou o login. Tente novamente.');
    }
    const reader = response.body?.getReader();
    if (!reader) throw new Error('O servidor devolveu uma resposta incompleta.');
    const chunks: Uint8Array[] = [];
    let bytes = 0;
    try {
      for (;;) {
        const part = await reader.read();
        if (part.done) break;
        bytes += part.value.byteLength;
        if (bytes > 32_768) throw new Error('O servidor devolveu uma resposta inválida.');
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
      throw new Error('O servidor devolveu uma resposta inválida.');
    }
  }
  return {
    async challenge(provider, client, signal) {
      const audience = audiences[provider]?.[client];
      if (!audience) throw new Error('Este login ainda não está configurado nesta versão.');
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
