import test from 'node:test';
import assert from 'node:assert/strict';
import { IDBFactory } from 'fake-indexeddb';
import { BrowserVault } from './vault.ts';
import type { StoredSession } from './session.ts';

const name = 'zoen-browser-v1';
function operation<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); });
}
function complete(transaction: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => { transaction.oncomplete = () => resolve(); transaction.onabort = () => reject(transaction.error); });
}
function install(t: test.TestContext) {
  const factory = new IDBFactory();
  const descriptor = Object.getOwnPropertyDescriptor(globalThis, 'indexedDB');
  Object.defineProperty(globalThis, 'indexedDB', { configurable: true, value: factory });
  t.after(() => { if (descriptor) Object.defineProperty(globalThis, 'indexedDB', descriptor); else Reflect.deleteProperty(globalThis, 'indexedDB'); });
  return factory;
}

test('canceling a save during encryption does not commit a session', async (t) => {
  const factory = install(t);
  const vault = await BrowserVault.open();
  t.after(() => vault.close());
  const controller = new AbortController();
  const saving = vault.save({ kind: 'empty' }, controller.signal);
  controller.abort();
  await assert.rejects(saving, { name: 'AbortError' });
  const db = await operation(factory.open(name));
  t.after(() => db.close());
  const transaction = db.transaction('vault', 'readonly');
  const done = complete(transaction);
  assert.equal(await operation(transaction.objectStore('vault').get('session')), undefined);
  await done;
});

test('the vault keeps protected signing keys and encrypted state across an IndexedDB reopen', async (t) => {
  const factory = install(t);
  const first = await BrowserVault.open();
  const device = await first.device();
  const session: StoredSession = { kind: 'pending', pending: { device, relay: 'https://relay.example', hpke_secret: '11'.repeat(32), secret: '22'.repeat(32),
    code: 'zoen-link:1:fixture', check: '123456', box_id: '33'.repeat(32), expires_at_ms: 0 } };
  await first.save(session); first.close();
  const second = await BrowserVault.open();
  t.after(() => second.close());
  assert.equal(await second.device(), device);
  assert.deepEqual(await second.load(), session);
  const publicKey = await crypto.subtle.importKey('raw', Uint8Array.from(device.match(/../g) ?? [], (byte) => Number.parseInt(byte, 16)), 'Ed25519', false, ['verify']);
  const payload = new TextEncoder().encode('reopened protected key');
  assert.equal(await crypto.subtle.verify('Ed25519', publicKey, await second.sign(payload), payload), true);
  const db = await operation(factory.open(name));
  t.after(() => db.close());
  const transaction = db.transaction('vault', 'readonly');
  const done = complete(transaction);
  const encrypted: unknown = await operation(transaction.objectStore('vault').get('session'));
  await done;
  assert.equal(JSON.stringify(encrypted).includes('relay.example'), false);
});

test('an orphaned encrypted session refuses key replacement without any mutation', async (t) => {
  const factory = install(t);
  const opening = factory.open(name, 1);
  opening.onupgradeneeded = () => opening.result.createObjectStore('vault');
  const db = await operation(opening);
  const transaction = db.transaction('vault', 'readwrite');
  const done = complete(transaction);
  const original = { iv: new Uint8Array(12), ciphertext: new Uint8Array([9, 8, 7]) };
  transaction.objectStore('vault').put(original, 'session');
  await done; db.close();
  const generated = t.mock.method(crypto.subtle, 'generateKey');
  await assert.rejects(BrowserVault.open(), /missing its protected keys/);
  assert.equal(generated.mock.calls.length, 0);
  const inspecting = await operation(factory.open(name));
  t.after(() => inspecting.close());
  const read = inspecting.transaction('vault', 'readonly');
  const readDone = complete(read);
  const keys = operation(read.objectStore('vault').get('keys'));
  const session = operation(read.objectStore('vault').get('session'));
  assert.equal(await keys, undefined);
  assert.deepEqual(await session, original);
  await readDone;
});

test('forgetting the vault deletes state and creates a distinct protected device on reopen', async (t) => {
  install(t);
  const original = await BrowserVault.open();
  const device = await original.device();
  await original.save({ kind: 'empty' });
  await original.erase();
  const replacement = await BrowserVault.open();
  t.after(() => replacement.close());
  assert.notEqual(await replacement.device(), device);
  assert.deepEqual(await replacement.load(), { kind: 'empty' });
});

test('erase fences a save suspended in encryption and cannot resurrect the old device', async (t) => {
  install(t);
  const vault = await BrowserVault.open();
  const device = await vault.device();
  let resume!: () => void;
  const suspended = new Promise<void>(resolve => { resume = resolve; });
  const encrypt = crypto.subtle.encrypt.bind(crypto.subtle);
  t.mock.method(crypto.subtle, 'encrypt', async (...args: Parameters<SubtleCrypto['encrypt']>) => {
    await suspended;
    return encrypt(...args);
  });
  const saving = vault.save({ kind: 'empty' });
  const refused = assert.rejects(saving, /vault is closed/);
  await vault.erase();
  resume();
  await refused;
  await assert.rejects(vault.sign(new Uint8Array([1])), /vault is closed/);
  assert.throws(() => vault.authorityKey(), /vault is closed/);
  const next = await BrowserVault.open();
  t.after(() => next.close());
  assert.notEqual(await next.device(), device);
  assert.deepEqual(await next.load(), { kind: 'empty' });
});

test('erase clears protected records despite another database connection and is idempotent', async (t) => {
  const factory = install(t);
  const vault = await BrowserVault.open();
  await vault.save({ kind: 'empty' });
  const inspection = await operation(factory.open(name));
  t.after(() => inspection.close());
  await Promise.all([vault.erase(), vault.erase()]);
  const read = inspection.transaction('vault', 'readonly');
  const done = complete(read);
  assert.deepEqual(await operation(read.objectStore('vault').getAllKeys()), []);
  await done;
});

test('failed durable erasure stays fenced and can retry without restoring signing access', async (t) => {
  install(t);
  const vault = await BrowserVault.open();
  const device = await vault.device();
  await vault.save({ kind: 'empty' });
  const db = (vault as unknown as { db: IDBDatabase }).db;
  const failure = t.mock.method(db, 'transaction', () => { throw new Error('Synthetic storage unavailable'); });
  await assert.rejects(vault.erase(), /storage unavailable/);
  await assert.rejects(vault.save({ kind: 'empty' }), /vault is closed/);
  await assert.rejects(vault.sign(new Uint8Array([2])), /vault is closed/);
  failure.mock.restore();
  await vault.erase();
  const next = await BrowserVault.open();
  t.after(() => next.close());
  assert.notEqual(await next.device(), device);
});

test('a signature already running at erasure cannot leave the closed vault', async (t) => {
  install(t);
  const vault = await BrowserVault.open();
  let resume!: () => void;
  const suspended = new Promise<void>(resolve => { resume = resolve; });
  const sign = crypto.subtle.sign.bind(crypto.subtle);
  t.mock.method(crypto.subtle, 'sign', async (...args: Parameters<SubtleCrypto['sign']>) => {
    await suspended;
    return sign(...args);
  });
  const signing = vault.sign(new Uint8Array([1, 2, 3]));
  const refused = assert.rejects(signing, /vault is closed/);
  await vault.erase();
  resume();
  await refused;
});
