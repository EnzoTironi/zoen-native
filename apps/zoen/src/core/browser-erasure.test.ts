import assert from 'node:assert/strict';
import { test } from 'node:test';
import { build } from 'esbuild';
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
  `data:text/javascript;base64,${Buffer.from(output.outputFiles[0]!.text).toString('base64')}`
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
