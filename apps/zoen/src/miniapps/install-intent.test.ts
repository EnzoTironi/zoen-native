import test from 'node:test';
import assert from 'node:assert/strict';
import { installIntent } from './install-intent.ts';
test('same immutable creation intent retains one token across ambiguous transport retries', async () => {
  const intent = installIntent();
  let ids = 0;
  const next = async () => `id-${++ids}`;
  const original = intent('epoch1/chatA/titleOne', next);
  assert.equal(intent('epoch1/chatA/titleOne', next), original);
  assert.equal(await original, 'id-1');
  assert.equal(await intent('epoch1/chatA/titleOne', next), 'id-1');
  assert.equal(await intent('epoch1/chatA/titleTwo', next), 'id-2');
  assert.equal(await intent('epoch1/chatB/titleTwo', next), 'id-3');
  assert.equal(await intent('epoch2/chatB/titleTwo', next), 'id-4');
});
test('failed token allocation is retryable and cannot evict a newer intent', async () => {
  const intent = installIntent();
  const failed = intent('old', async () => {
    throw Error('allocation failed');
  });
  const current = intent('new', async () => 'new-id');
  await assert.rejects(failed);
  assert.equal(await current, 'new-id');
  assert.equal(await intent('new', async () => 'wrong'), 'new-id');
  assert.equal(await intent('old', async () => 'old-id'), 'old-id');
});
