import assert from 'node:assert/strict';
import { test } from 'node:test';
import { completeProviderSession } from './complete-session.ts';
import type { VerifiedSignIn } from './types.ts';

const proof: VerifiedSignIn = {
  provider: 'google',
  proof_token: 'a'.repeat(64),
  expires_at_ms: Date.now() + 300_000,
  account: { kind: 'unbound' },
};
test('recognized account on another device chooses recovery before creating keys', async () => {
  let recovery: unknown;
  await completeProviderSession(
    { ...proof, account: { kind: 'bound', identity_id: 'existing', handle: 'person' } },
    undefined,
    () => {
      throw new Error('must not provision');
    },
    (account) => {
      recovery = account;
    },
  );
  assert.deepEqual(recovery, { identityId: 'existing', handle: 'person' });
  assert.equal(JSON.stringify(recovery).includes(proof.proof_token), false);
});
test('pending local identity can finish the same binding after a lost response', async () => {
  let calls = 0;
  await completeProviderSession(
    {
      ...proof,
      account: { kind: 'bound', identity_id: 'pending', handle: 'person' },
      suggested_name: 'Ana',
    },
    'pending',
    async (token, name) => {
      assert.equal(token, proof.proof_token);
      assert.equal(name, 'Ana');
      calls++;
    },
    () => {
      throw new Error('must not recover');
    },
  );
  assert.equal(calls, 1);
});
test('expired proof cannot create keys or choose recovery', async () => {
  await assert.rejects(
    () =>
      completeProviderSession(
        { ...proof, expires_at_ms: 1 },
        undefined,
        async () => {
          throw new Error('must not provision');
        },
        () => {
          throw new Error('must not recover');
        },
      ),
    /Confirme/,
  );
});
