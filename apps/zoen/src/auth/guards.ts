import type { AuthChallenge, AuthProvider, ProviderCredential, AuthVerification } from './types';

export class AuthCancelled extends Error {
  constructor() {
    super('Login cancelado.');
    this.name = 'AuthCancelled';
  }
}
export function cancelled(reason: unknown): boolean {
  return (
    reason instanceof AuthCancelled || (reason instanceof Error && reason.name === 'AbortError')
  );
}
const record = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === 'object' && !Array.isArray(value);
const opaque = (value: unknown): value is string =>
  typeof value === 'string' && /^[A-Za-z0-9_-]{16,256}$/.test(value);

export function challengeFromWire(
  value: unknown,
  expectedAudience: string,
  now = Date.now(),
): AuthChallenge {
  if (
    !record(value) ||
    !opaque(value.id) ||
    typeof value.nonce !== 'string' ||
    !/^[a-f0-9]{64}$/.test(value.nonce) ||
    !opaque(value.state) ||
    value.audience !== expectedAudience ||
    typeof value.expires_ms !== 'number' ||
    !Number.isSafeInteger(value.expires_ms) ||
    value.expires_ms <= now ||
    value.expires_ms > now + 5 * 60_000 + 10_000
  )
    throw new Error('Não foi possível preparar um login seguro. Tente novamente.');
  return Object.freeze({
    challenge_id: value.id,
    nonce: value.nonce,
    state: value.state,
    audience: expectedAudience,
    expires_at_ms: value.expires_ms,
  });
}

export function verificationFromWire(value: unknown, now = Date.now()): AuthVerification {
  if (
    !record(value) ||
    !opaque(value.proof_token) ||
    typeof value.expires_ms !== 'number' ||
    !Number.isSafeInteger(value.expires_ms) ||
    value.expires_ms <= now ||
    value.expires_ms > now + 5 * 60_000 + 10_000 ||
    !record(value.account)
  )
    throw new Error('Não foi possível confirmar o login. Tente novamente.');
  const account = value.account;
  let normalized: AuthVerification['account'];
  if (account.status === 'unbound') normalized = { kind: 'unbound' };
  else if (
    account.status === 'bound' &&
    typeof account.identity_id === 'string' &&
    /^[a-zA-Z0-9:_-]{16,256}$/.test(account.identity_id) &&
    typeof account.handle === 'string' &&
    /^[a-z][a-z0-9._]{2,23}$/.test(account.handle)
  )
    normalized = { kind: 'bound', identity_id: account.identity_id, handle: account.handle };
  else throw new Error('A conta recebida não pôde ser confirmada.');
  const name = profileName(value.name);
  return {
    proof_token: value.proof_token,
    expires_at_ms: value.expires_ms,
    account: normalized,
    ...(name ? { profile: { name } } : {}),
  };
}

export function profileName(value: unknown): string | undefined {
  if (typeof value !== 'string') return undefined;
  return (
    [...value]
      .filter((char) => char.charCodeAt(0) >= 32)
      .join('')
      .trim()
      .slice(0, 200) || undefined
  );
}

function untrustedJwtPayload(token: string): Record<string, unknown> {
  if (token.length > 16_384 || !/^[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$/.test(token))
    throw new Error('O provedor não devolveu uma credencial válida.');
  // Bounded base64url decoding only. This deliberately does NOT verify a JWT.
  // Avoid assuming atob exists in Hermes; the server still verifies the signature.
  const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_';
  if (token.split('.')[1]!.length % 4 === 1) throw new Error('A credencial recebida é inválida.');
  let accumulator = 0,
    bits = 0,
    encoded = '';
  for (const char of token.split('.')[1]!) {
    accumulator = (accumulator << 6) | alphabet.indexOf(char);
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      encoded += `%${((accumulator >> bits) & 255).toString(16).padStart(2, '0')}`;
    }
  }
  if (bits && accumulator & ((1 << bits) - 1)) throw new Error('A credencial recebida é inválida.');
  try {
    const value: unknown = JSON.parse(decodeURIComponent(encoded));
    if (record(value)) return value;
  } catch {
    /* Never surface the token or decoded provider payload in an error. */
  }
  throw new Error('A credencial recebida é inválida.');
}

/** Reject session mixups locally; only server signature verification authenticates. */
export function guardCredentialContext(
  provider: AuthProvider,
  challenge: AuthChallenge,
  credential: ProviderCredential,
  requireEchoedState: boolean,
  now = Date.now(),
): void {
  if (challenge.expires_at_ms <= now) throw new Error('O login expirou. Tente novamente.');
  if (requireEchoedState && credential.state !== challenge.state)
    throw new Error('A resposta não pertence a este login.');
  if (credential.state !== undefined && credential.state !== challenge.state)
    throw new Error('A resposta não pertence a este login.');
  const payload = untrustedJwtPayload(credential.id_token);
  const issuers =
    provider === 'apple'
      ? ['https://appleid.apple.com']
      : ['https://accounts.google.com', 'accounts.google.com'];
  if (
    !issuers.includes(String(payload.iss)) ||
    payload.aud !== challenge.audience ||
    payload.nonce !== challenge.nonce ||
    typeof payload.exp !== 'number' ||
    !Number.isSafeInteger(payload.exp) ||
    payload.exp * 1000 <= now
  )
    throw new Error('A credencial não corresponde ao login solicitado.');
  if (
    provider === 'apple' &&
    (!credential.authorization_code || credential.authorization_code.length > 4096)
  )
    throw new Error('A Apple não devolveu o código de autorização. Tente novamente.');
}

export function serviceOrigin(input: string): string {
  const url = new URL(input);
  const local = ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname);
  if (
    (url.protocol !== 'https:' && !(local && url.protocol === 'http:')) ||
    url.username ||
    url.password ||
    url.search ||
    url.hash ||
    (url.pathname !== '/' && url.pathname !== '')
  )
    throw new Error('O servidor de login precisa usar uma origem HTTPS válida.');
  return url.origin;
}

export function webAppleRedirect(input: string, origin: string): string {
  const uri = new URL(input);
  if (
    uri.protocol !== 'https:' ||
    uri.origin !== origin ||
    uri.username ||
    uri.password ||
    uri.hash ||
    uri.search
  )
    throw new Error('O retorno Apple precisa pertencer a este site HTTPS.');
  return uri.href;
}
