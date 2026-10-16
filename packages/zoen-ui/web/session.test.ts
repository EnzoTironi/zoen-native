import test from 'node:test';
import assert from 'node:assert/strict';
import { inviteFromPath, parseSession, relayUrl, socketUrl, type StoredSession } from './session.ts';
import { openSession, sealSession } from './vault.ts';
import { persistBeforeOutgoing } from './persistence.ts';

test('relay origins require TLS remotely and preserve the native sync endpoint', () => {
  assert.equal(socketUrl('https://relay.example/'), 'wss://relay.example/v1/sync');
  assert.equal(socketUrl('http://127.0.0.1:9000'), 'ws://127.0.0.1:9000/v1/sync');
  for (const invalid of ['http://relay.example', 'https://u:p@relay.example', 'https://relay.example/a', 'https://relay.example/?x=1']) {
    assert.throws(() => relayUrl(invalid));
  }
  assert.throws(() => parseSession('{"kind":"linked","account":{}}'));
  assert.equal(inviteFromPath('/join/abc1234567'), 'ABC1234567');
  assert.equal(inviteFromPath('/join/ILOU123456'), null);
  assert.equal(inviteFromPath('/join/1234567890/extra'), null);
});

test('nonexportable WebCrypto keys sign and authenticate encrypted session snapshots', async () => {
  const signer = await crypto.subtle.generateKey('Ed25519', false, ['sign', 'verify']);
  assert.ok('privateKey' in signer);
  assert.equal(signer.privateKey.extractable, false);
  await assert.rejects(crypto.subtle.exportKey('pkcs8', signer.privateKey));
  const payload = new TextEncoder().encode('zoen-sync/2:auth:test:nonce');
  const signature = await crypto.subtle.sign('Ed25519', signer.privateKey, payload);
  assert.equal(await crypto.subtle.verify('Ed25519', signer.publicKey, signature, payload), true);
  const key = await crypto.subtle.generateKey({ name: 'AES-GCM', length: 256 }, false, ['encrypt', 'decrypt']);
  await assert.rejects(crypto.subtle.exportKey('raw', key));
  const session: StoredSession = { kind: 'pending', pending: {
    device: '11'.repeat(32), relay: 'https://relay.example', hpke_secret: '22'.repeat(32), secret: '33'.repeat(32),
    code: 'zoen-link:1:' + '11'.repeat(32) + '22'.repeat(32) + '33'.repeat(32), check: '123456', box_id: '44'.repeat(32), expires_at_ms: 0,
  } };
  const sealed = await sealSession(session, key);
  assert.deepEqual(await openSession(sealed, key), session);
  assert.ok(!new TextDecoder().decode(sealed.ciphertext).includes('relay.example'));
  const again = await sealSession(session, key);
  assert.notDeepEqual(again.iv, sealed.iv);
  const tampered = { ...sealed, ciphertext: Uint8Array.from(sealed.ciphertext) };
  const first = tampered.ciphertext[0];
  assert.notEqual(first, undefined);
  tampered.ciphertext[0] = (first ?? 0) ^ 1;
  await assert.rejects(openSession(tampered, key));
});

test('a failed durable save sends no outgoing MLS frame', async () => {
  const emitted: Uint8Array[] = [];
  await assert.rejects(persistBeforeOutgoing({ snapshot: 'new MLS state', save: async () => {
    throw new Error('QuotaExceededError');
  }, frames: [new Uint8Array([1]), new Uint8Array([2])], emit: (frame) => emitted.push(frame) }), /QuotaExceededError/);
  assert.deepEqual(emitted, []);
});

test('outgoing frames wait until the actual save promise completes', async () => {
  const order: string[] = [];
  let durable = () => {};
  const saved = new Promise<void>((resolve) => { durable = resolve; });
  const sending = persistBeforeOutgoing({ snapshot: 'epoch 7', save: async (snapshot) => {
    assert.equal(snapshot, 'epoch 7'); order.push('save started'); await saved; order.push('save completed');
  }, frames: [new Uint8Array([7])], emit: () => order.push('sent') });
  await Promise.resolve();
  assert.deepEqual(order, ['save started']);
  durable(); await sending;
  assert.deepEqual(order, ['save started', 'save completed', 'sent']);
});
