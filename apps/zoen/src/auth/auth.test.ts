import test from 'node:test';
import assert from 'node:assert/strict';
import { AuthAttempt } from './attempt.ts';
import { createAuthService } from './client.ts';
import {
  AuthCancelled,
  challengeFromWire,
  guardCredentialContext,
  verificationFromWire,
  serviceOrigin,
  webAppleRedirect,
} from './guards.ts';
import { authAudiences, providerConfiguration } from './configuration.ts';
import type { AuthChallenge, AuthService, AuthVerification } from './types';

const now = Date.now();
const challenge = (): AuthChallenge => ({
  challenge_id: 'a'.repeat(64),
  nonce: 'b'.repeat(64),
  state: 'c'.repeat(64),
  audience: '123-example.apps.googleusercontent.com',
  expires_at_ms: Date.now() + 300_000,
});
const proof = (): AuthVerification => ({
  proof_token: 'd'.repeat(64),
  expires_at_ms: Date.now() + 300_000,
  account: { kind: 'unbound' },
});
const jwt = (payload: Record<string, unknown>) =>
  `${Buffer.from('{"alg":"RS256","kid":"test"}').toString('base64url')}.${Buffer.from(JSON.stringify(payload)).toString('base64url')}.c2lnbmF0dXJl`;
const token = (input: AuthChallenge, patch: Record<string, unknown> = {}) =>
  jwt({
    iss: 'https://accounts.google.com',
    aud: input.audience,
    nonce: input.nonce,
    exp: Math.floor(Date.now() / 1000) + 300,
    sub: '123456',
    name: 'João 👋',
    ...patch,
  });

test('challenge and verified account parsers reject audience mixups, expiry and ambiguous bindings', () => {
  const wire = {
    id: 'a'.repeat(64),
    nonce: 'b'.repeat(64),
    state: 'c'.repeat(64),
    audience: 'expected',
    expires_ms: now + 300_000,
  };
  assert.equal(challengeFromWire(wire, 'expected', now).audience, 'expected');
  assert.throws(() => challengeFromWire(wire, 'other', now));
  assert.throws(() => challengeFromWire({ ...wire, nonce: 'old' }, 'expected', now));
  assert.throws(() => challengeFromWire({ ...wire, expires_ms: now }, 'expected', now));
  assert.throws(() => challengeFromWire({ ...wire, expires_ms: now + 3_600_000 }, 'expected', now));
  const verified = {
    proof_token: 'd'.repeat(64),
    expires_ms: now + 300_000,
    account: { status: 'bound', identity_id: 'e'.repeat(64), handle: 'ana' },
    name: ' Ana ',
  };
  assert.deepEqual(verificationFromWire(verified, now).account, {
    kind: 'bound',
    identity_id: 'e'.repeat(64),
    handle: 'ana',
  });
  assert.equal(verificationFromWire(verified, now).profile?.name, 'Ana');
  assert.throws(() =>
    verificationFromWire(
      { ...verified, account: { status: 'bound', email: 'same@example.com' } },
      now,
    ),
  );
});

test('unverified JWT context guards reject wrong nonce, audience, issuer, expired token and state before backend verification', () => {
  const input = challenge();
  const credential = { id_token: token(input), state: input.state };
  assert.doesNotThrow(() => guardCredentialContext('google', input, credential, true));
  for (const patch of [
    { nonce: 'replayed' },
    { aud: 'another-client' },
    { iss: 'https://attacker.example' },
    { exp: 1 },
  ])
    assert.throws(() =>
      guardCredentialContext(
        'google',
        input,
        { ...credential, id_token: token(input, patch) },
        true,
      ),
    );
  assert.throws(() =>
    guardCredentialContext('google', input, { ...credential, state: 'another-attempt' }, true),
  );
  assert.throws(() =>
    guardCredentialContext('google', input, { id_token: credential.id_token }, true),
  );
  assert.throws(() =>
    guardCredentialContext('google', input, { ...credential, id_token: 'a.'.repeat(20_000) }, true),
  );
  const apple = { ...input, audience: 'xyz.tironi.zoen' };
  const appleToken = token(apple, { iss: 'https://appleid.apple.com' });
  assert.throws(() =>
    guardCredentialContext('apple', apple, { id_token: appleToken, state: apple.state }, true),
  );
  assert.doesNotThrow(() =>
    guardCredentialContext(
      'apple',
      apple,
      { id_token: appleToken, authorization_code: 'one-use-code', state: apple.state },
      true,
    ),
  );
});

test('only an owned live ticket can verify once; decoding a matching JWT never bypasses the server', async () => {
  let calls = 0;
  const service: AuthService = {
    challenge: async () => challenge(),
    verify: async () => {
      calls++;
      return proof();
    },
  };
  const attempt = new AuthAttempt(service);
  const ticket = await attempt.prepare('google', 'web');
  const credential = { id_token: token(ticket.challenge), state: ticket.challenge.state };
  await assert.rejects(attempt.complete({ ...ticket }, credential), AuthCancelled);
  assert.equal(calls, 0);
  const result = await attempt.complete(ticket, credential, ' João ');
  assert.equal(calls, 1);
  assert.equal(result.suggested_name, 'João');
  assert.equal(result.provider, 'google');
  await assert.rejects(attempt.complete(ticket, credential), AuthCancelled);
  assert.equal(calls, 1);
  attempt.dispose();
});

test('account change/unmount discards late provider verification and pending challenge without publishing a proof', async () => {
  let finish: ((value: AuthVerification) => void) | undefined;
  const service: AuthService = {
    challenge: async () => challenge(),
    verify: () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  };
  const attempt = new AuthAttempt(service);
  const ticket = await attempt.prepare('google', 'ios');
  const completing = attempt.complete(ticket, { id_token: token(ticket.challenge) });
  attempt.dispose();
  attempt.activate();
  finish!(proof());
  await assert.rejects(completing, AuthCancelled);
  const next = await attempt.prepare('google', 'ios');
  assert.equal(next.signal.aborted, false);
  attempt.dispose();
  assert.equal(next.signal.aborted, true);
});

test('transport uses explicit origin/POST/abort, carries exact challenge context, and refuses redirects or oversized replies', async () => {
  const requests: { url: string; init: RequestInit }[] = [];
  let large = false;
  const mock: typeof fetch = async (input, init) => {
    requests.push({ url: String(input), init: init! });
    return new Response(
      large
        ? 'a'.repeat(40_000)
        : JSON.stringify({
            id: 'a'.repeat(64),
            nonce: 'b'.repeat(64),
            state: 'c'.repeat(64),
            audience: 'audience',
            expires_ms: Date.now() + 300_000,
          }),
      { headers: { 'Content-Type': 'application/json' } },
    );
  };
  const api = createAuthService('https://relay.tryzoen.com', { google: { web: 'audience' } }, mock);
  const controller = new AbortController();
  const result = await api.challenge('google', 'web', controller.signal);
  assert.equal(result.nonce, 'b'.repeat(64));
  assert.equal(requests[0]!.url, 'https://relay.tryzoen.com/v1/auth/challenge');
  assert.equal(requests[0]!.init.redirect, 'error');
  assert.equal(requests[0]!.init.credentials, 'omit');
  assert.equal(requests[0]!.init.signal, controller.signal);
  assert.deepEqual(JSON.parse(String(requests[0]!.init.body)), {
    provider: 'google',
    client: 'web',
  });
  large = true;
  await assert.rejects(api.challenge('google', 'web', controller.signal), /inválida/);
  assert.throws(() => serviceOrigin('http://example.com'));
  assert.throws(() => serviceOrigin('https://user:secret@example.com'));
  assert.throws(() => serviceOrigin('https://relay.tryzoen.com/?token=secret'));
  assert.equal(serviceOrigin('http://127.0.0.1:8080'), 'http://127.0.0.1:8080');
});

test('public config remains missing when invalid and Apple popup return is exact same HTTPS origin', () => {
  assert.deepEqual(providerConfiguration({}, undefined), {});
  assert.equal(
    providerConfiguration({ googleWebClientId: 'secret-not-client-id' }, undefined).google,
    undefined,
  );
  const config = providerConfiguration(
    {
      googleWebClientId: '123-abc.apps.googleusercontent.com',
      appleServiceId: 'xyz.tironi.zoen.web',
    },
    'xyz.tironi.zoen',
  );
  assert.equal(authAudiences(config).google?.ios, '123-abc.apps.googleusercontent.com');
  assert.equal(authAudiences(config).apple?.web, 'xyz.tironi.zoen.web');
  assert.equal(
    webAppleRedirect('https://app.tryzoen.com/auth/apple', 'https://app.tryzoen.com'),
    'https://app.tryzoen.com/auth/apple',
  );
  for (const uri of [
    'https://attacker.example/auth',
    'http://app.tryzoen.com/auth',
    'https://app.tryzoen.com/auth?redirect=foreign',
  ])
    assert.throws(() => webAppleRedirect(uri, 'https://app.tryzoen.com'));
});
