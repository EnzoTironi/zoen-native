import test from 'node:test';
import assert from 'node:assert/strict';
import type {
  GoogleSignInError,
  OneTapConfigureParams,
  OneTapResponse,
} from 'react-native-nitro-google-signin';
import { AuthAttempt } from './attempt.ts';
import { createAuthService } from './client.ts';
import { authAudiences, providerConfiguration } from './configuration.ts';
import { AuthCancelled } from './guards.ts';
import {
  nativeAuthClient,
  nativeGoogleConfigured,
  nativeGoogleCredential,
  type GoogleCredentialSdk,
} from './native-google.ts';
import type { AuthChallenge, AuthClient, AuthService, ProviderConfiguration } from './types';

const configuration: ProviderConfiguration = {
  google: {
    webClientId: '123-server.apps.googleusercontent.com',
    iosClientId: '123-ios.apps.googleusercontent.com',
    androidClientId: '123-android.apps.googleusercontent.com',
  },
};
const challenge = (): AuthChallenge => ({
  challenge_id: 'aa'.repeat(32),
  nonce: 'bb'.repeat(32),
  state: 'cc'.repeat(32),
  audience: configuration.google!.webClientId,
  expires_at_ms: Date.now() + 300_000,
});
const jwt = (nonce: string) =>
  `${Buffer.from('{"alg":"RS256","kid":"test"}').toString('base64url')}.${Buffer.from(
    JSON.stringify({
      iss: 'https://accounts.google.com',
      aud: configuration.google!.webClientId,
      azp: configuration.google!.androidClientId,
      nonce,
      exp: Math.floor(Date.now() / 1000) + 300,
      sub: 'synthetic-person',
    }),
  ).toString('base64url')}.c2lnbmF0dXJl`;
const success = (nonce: string): OneTapResponse => ({
  type: 'success',
  data: {
    idToken: jwt(nonce),
    user: {
      id: 'synthetic-person',
      email: null,
      name: ' Ana ',
      givenName: null,
      familyName: null,
      photo: null,
    },
    scopes: [],
    serverAuthCode: null,
  },
});
function sdk(present: () => Promise<OneTapResponse>) {
  const options: OneTapConfigureParams[] = [];
  let cleared = 0;
  const api: GoogleCredentialSdk = {
    GoogleOneTapSignIn: {
      configure: (value) => options.push(value),
      presentExplicitSignIn: present,
      signOut: async () => {
        cleared++;
      },
    },
    statusCodes: { SIGN_IN_CANCELLED: 'SIGN_IN_CANCELLED' },
    isErrorWithCode: (value): value is Pick<GoogleSignInError, 'message' | 'code' | 'userInfo'> =>
      value instanceof Error && 'code' in value,
    isCancelledResponse: (value): value is OneTapResponse & { type: 'cancelled' } =>
      value.type === 'cancelled',
    isSuccessResponse: (
      value,
    ): value is OneTapResponse & { type: 'success'; data: NonNullable<OneTapResponse['data']> } =>
      value.type === 'success' && value.data !== null,
  };
  return { api, options, cleared: () => cleared };
}
function service() {
  const contexts: AuthClient[] = [];
  let verifications = 0;
  const api: AuthService = {
    challenge: async (_provider, client) => {
      contexts.push(client);
      return challenge();
    },
    verify: async (_challenge, credential) => {
      verifications++;
      assert.equal(credential.state, 'cc'.repeat(32));
      return {
        proof_token: 'dd'.repeat(32),
        expires_at_ms: Date.now() + 300_000,
        account: { kind: 'unbound' },
      };
    },
  };
  return { api, contexts, verifications: () => verifications };
}

test('native provider registration is platform-specific and Android never inherits an iOS ID', () => {
  assert.equal(nativeAuthClient('ios'), 'ios');
  assert.equal(nativeAuthClient('android'), 'android');
  assert.equal(nativeAuthClient('web'), null);
  assert.equal(nativeAuthClient('windows'), null);
  const iosOnly = {
    google: {
      webClientId: configuration.google!.webClientId,
      iosClientId: configuration.google!.iosClientId,
    },
  };
  assert.equal(nativeGoogleConfigured(iosOnly, 'ios'), true);
  assert.equal(nativeGoogleConfigured(iosOnly, 'android'), false);
  assert.equal(authAudiences(iosOnly).google?.android, undefined);
  assert.equal(
    nativeGoogleConfigured(
      {
        google: {
          webClientId: configuration.google!.webClientId,
          androidClientId: configuration.google!.androidClientId,
        },
      },
      'ios',
    ),
    false,
  );
  const configured = providerConfiguration(
    {
      googleWebClientId: configuration.google!.webClientId,
      googleAndroidClientId: configuration.google!.androidClientId,
    },
    undefined,
  );
  assert.equal(authAudiences(configured).google?.android, configuration.google!.webClientId);
  assert.equal(authAudiences(configured).apple?.android, undefined);
  assert.equal(
    providerConfiguration(
      { googleWebClientId: configuration.google!.webClientId, googleAndroidClientId: 'invalid' },
      undefined,
    ).google?.androidClientId,
    undefined,
  );
});

for (const client of ['ios', 'android'] as const) {
  test(`${client} explicit Google flow preserves the server nonce, requests no extra scopes and clears the SDK session`, async () => {
    const backend = service();
    const attempt = new AuthAttempt(backend.api);
    const ticket = await attempt.prepare('google', client);
    const google = sdk(async () => success(ticket.challenge.nonce));
    const credential = await nativeGoogleCredential(ticket, google.api, configuration);
    const verified = await attempt.complete(
      ticket,
      credential.credential,
      credential.suggestedName,
    );
    assert.deepEqual(backend.contexts, [client]);
    assert.equal(backend.verifications(), 1);
    assert.equal(verified.suggested_name, 'Ana');
    assert.deepEqual(google.options, [
      {
        webClientId: configuration.google!.webClientId,
        ...(client === 'ios' ? { iosClientId: configuration.google!.iosClientId } : {}),
        nonce: ticket.challenge.nonce,
        scopes: [],
        offlineAccess: false,
        autoSelectOnSignIn: false,
      },
    ]);
    assert.equal(google.cleared(), 1);
    assert.equal(ticket.signal.aborted, true);
  });
}

test('Android HTTP challenge carries android context and cannot run without its own registration', async () => {
  let body: unknown;
  const configured = createAuthService(
    'https://relay.tryzoen.com',
    authAudiences(configuration),
    async (_url, init) => {
      body = JSON.parse(String(init?.body));
      const value = challenge();
      return Response.json({
        id: value.challenge_id,
        nonce: value.nonce,
        state: value.state,
        audience: value.audience,
        expires_ms: value.expires_at_ms,
      });
    },
  );
  await configured.challenge('google', 'android', new AbortController().signal);
  assert.deepEqual(body, { provider: 'google', client: 'android' });
  const ticket = await new AuthAttempt(service().api).prepare('google', 'android');
  const google = sdk(async () => success(ticket.challenge.nonce));
  await assert.rejects(
    nativeGoogleCredential(ticket, google.api, {
      google: {
        webClientId: configuration.google!.webClientId,
        iosClientId: configuration.google!.iosClientId,
      },
    }),
  );
  assert.deepEqual(google.options, []);
  assert.equal(google.cleared(), 0);
});

test('cancelled, failed and late Android credentials never become a Zoen provider proof', async () => {
  for (const kind of ['cancel', 'error', 'aborted'] as const) {
    const backend = service();
    const attempt = new AuthAttempt(backend.api);
    const ticket = await attempt.prepare('google', 'android');
    const google = sdk(async () => {
      if (kind === 'cancel') return { type: 'cancelled', data: null };
      if (kind === 'error') throw new Error('provider failed');
      attempt.dispose();
      return success(ticket.challenge.nonce);
    });
    await assert.rejects(
      nativeGoogleCredential(ticket, google.api, configuration),
      kind === 'error' ? Error : AuthCancelled,
    );
    assert.equal(google.cleared(), 1);
    assert.equal(backend.verifications(), 0);
    attempt.dispose();
  }
});

test('a wrong Android nonce is refused before the server verifier is called', async () => {
  const backend = service();
  const attempt = new AuthAttempt(backend.api);
  const ticket = await attempt.prepare('google', 'android');
  const google = sdk(async () => success('old-nonce'));
  const credential = await nativeGoogleCredential(ticket, google.api, configuration);
  await assert.rejects(attempt.complete(ticket, credential.credential));
  assert.equal(backend.verifications(), 0);
  assert.equal(google.cleared(), 1);
  assert.equal(ticket.signal.aborted, true);
});
