import { build } from 'esbuild';
import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { CoreDriver } from './types.ts';

// Bundle the real platform driver so Node uses the same extension resolution as
// Metro. The fixture replaces only platform resources, never the erase method.
const output = await build({
  entryPoints: [new URL('./driver.web.ts', import.meta.url).pathname],
  bundle: true,
  platform: 'node',
  format: 'esm',
  write: false,
});
const module = await import(
  `data:text/javascript;base64,${Buffer.from(output.outputFiles[0]!.text + '\n//# sourceURL=zoen-browser-driver.fixture.js').toString('base64')}`
);
const scope = { identity_id: 'synthetic-owner', device_id: 'synthetic-device' };
function fixture(erase: () => Promise<void>) {
  const driver = module.createDriver() as CoreDriver;
  const fields = driver as unknown as Record<string, unknown>;
  const events: string[] = [];
  const controller = new AbortController();
  fields.session = {
    kind: 'linked',
    account: { identity: scope.identity_id, device: scope.device_id },
  };
  fields.vault = { erase, close: () => events.push('vault-close') };
  fields.providerAbort = controller;
  fields.disposeMedia = () => events.push('media');
  fields.pairing = { stop: () => events.push('pairing') };
  fields.messages = { stop: () => events.push('transport') };
  fields.worker = { close: () => events.push('worker') };
  fields.locked = new Promise<void>((resolve) => {
    fields.release = () => {
      events.push('release-lock');
      resolve();
    };
  });
  return { driver, fields, events, controller };
}

test('browser erasure cancels resources and keeps its exclusive lock until durable deletion', async () => {
  let resume!: () => void;
  const barrier = new Promise<void>((resolve) => {
    resume = resolve;
  });
  const { driver, fields, events, controller } = fixture(() => barrier);
  const erasing = driver.eraseDevice(scope);
  const closing = driver.close();
  assert.equal(controller.signal.aborted, true);
  assert.deepEqual(events, ['media', 'pairing', 'transport', 'worker']);
  await assert.rejects(driver.request({ op: 'snapshot' }), /sendo apagado/);
  resume();
  await Promise.all([erasing, closing]);
  assert.equal(events.filter((event) => event === 'release-lock').length, 1);
  assert.deepEqual(fields.session, { kind: 'empty' });
  await driver.eraseDevice(scope);
  await assert.rejects(driver.eraseDevice({ ...scope, device_id: 'other' }), /conta mudou/);
});

test('failed browser erasure remains closed and retryable without releasing the lock', async () => {
  let fail = true;
  const { driver, events } = fixture(async () => {
    if (fail) throw new Error('Synthetic durable storage failure');
  });
  await assert.rejects(driver.eraseDevice(scope), /storage failure/);
  await assert.rejects(driver.close(), /novamente/);
  await assert.rejects(driver.request({ op: 'snapshot' }), /sendo apagado/);
  assert.equal(events.includes('release-lock'), false);
  fail = false;
  await driver.eraseDevice(scope);
  assert.equal(events.filter((event) => event === 'release-lock').length, 1);
});

test('an old erase intent cannot affect a different live browser account', async () => {
  let calls = 0;
  const { driver, fields, events } = fixture(async () => {
    calls++;
  });
  fields.erasedScope = scope;
  fields.session = { kind: 'linked', account: { identity: 'new-owner', device: 'new-device' } };
  await assert.rejects(driver.eraseDevice(scope), /conta mudou/);
  assert.equal(calls, 0);
  assert.deepEqual(events, []);
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

test('delegated device projection reads the live device after transport settles', async () => {
  const { driver, fields } = fixture(async () => {});
  const gate = deferred<unknown>();
  const first = '11'.repeat(32),
    second = '22'.repeat(32);
  fields.session = { kind: 'linked', account: { identity: scope.identity_id, device: first } };
  fields.messages = { devices: () => gate.promise };
  const pending = driver.request({ op: 'devices' });
  // A controlled port projection change isolates the getter order, not a new-session admission.
  fields.session = { kind: 'linked', account: { identity: scope.identity_id, device: second } };
  gate.resolve({
    kind: 'devices',
    devices: [
      { device: first, revoked: false },
      { device: second, revoked: false },
    ],
  });
  assert.deepEqual(
    (await pending).map((value) => value.this_device),
    [false, true],
  );
});

test('delegated handle result reads the current account only after the write completes', async () => {
  const { driver, fields } = fixture(async () => {});
  const gate = deferred<void>();
  fields.messages = { updateHandle: () => gate.promise };
  const pending = driver.request({ op: 'update_handle', handle: 'new_handle' });
  fields.session = {
    kind: 'linked',
    account: {
      identity: scope.identity_id,
      device: scope.device_id,
      handle: 'new_handle',
      relay: 'https://qa.example',
      name: 'Synthetic',
    },
  };
  gate.resolve();
  assert.equal((await pending)?.handle, 'new_handle');
});

for (const operation of ['devices', 'update_handle'] as const) {
  test(`closing the real driver retires a delegated ${operation} result after await`, async () => {
    const { driver, fields } = fixture(async () => {});
    const gate = deferred<unknown>();
    fields.messages = { stop() {}, devices: () => gate.promise, updateHandle: () => gate.promise };
    const pending = driver.request(
      operation === 'devices' ? { op: 'devices' } : { op: 'update_handle', handle: 'queued' },
    );
    const refused = assert.rejects(pending, /conta mudou/);
    await driver.close();
    gate.resolve(operation === 'devices' ? { kind: 'devices', devices: [] } : undefined);
    await refused;
    assert.equal(fields.selfProfile, null);
  });
}

test('even an already computed delegated read is fenced across the added async boundary', async () => {
  const { driver, fields } = fixture(async () => {});
  fields.state = { kind: 'active', conversations: [{ id: 'empty', messages: [] }] };
  const pending = driver.request({ op: 'timeline', space_id: 'empty', before: null, limit: 20 });
  const refused = assert.rejects(pending, /conta mudou/);
  await driver.close();
  await refused;
  assert.deepEqual(fields.session, { kind: 'empty' });
});

test('profile refresh does not reuse an old linked session after its first await', async () => {
  const { driver, fields } = fixture(async () => {});
  const gate = deferred<unknown>();
  let reads = 0;
  fields.messages = {
    stop() {},
    profileRequest: () => {
      reads++;
      return gate.promise;
    },
  };
  const pending = driver.request({
    op: 'update_profile',
    name: 'Reviewed',
    bio: 'Exact',
    photo_sha256: null,
  });
  const refused = assert.rejects(pending, /conta mudou/);
  await driver.close();
  gate.resolve(null);
  await refused;
  assert.equal(reads, 1, 'No profile query is authorized by the retired linked session');
  assert.equal(fields.selfProfile, null);
});

test('unsupported enrollment remains closed without borrowing a transport method', async () => {
  const { driver } = fixture(async () => {});
  await assert.rejects(
    driver.request({
      op: 'agent_enrollment_status',
      expected_identity_id: scope.identity_id,
      expected_device_id: scope.device_id,
    }),
    /integrado ao cliente web/,
  );
});
