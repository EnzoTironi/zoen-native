import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { Miniflare, convertV4MiniflareOptions } from 'miniflare';
import { initSync, directoryKeyPackageReply, directoryReadReply } from '../generated/wasm/admission.js';

const root = fileURLToPath(new URL('../', import.meta.url));
const native = JSON.parse(await readFile(process.env.ZOEN_DIRECTORY_FIXTURE
  ? resolve(process.env.ZOEN_DIRECTORY_FIXTURE) : join(root, 'generated/directory-fixture.json'), 'utf8'));
assert.equal(native.schema, 1);
assert.equal(native.steps.length, 71);
assert.ok(native.reads.steps.length >= 65, 'The real native SQL read timeline is required');
initSync({ module: new WebAssembly.Module(await readFile(join(root, 'generated/wasm/admission_bg.wasm'))) });
function runtime(path) {
  return new Miniflare(convertV4MiniflareOptions({
    modulesRoot: root,
    modules: ['tests/directory-fixture.mjs', 'src/directory-state.mjs', 'src/relay-composition.mjs', 'src/provider-auth.mjs',
      'src/media-blobs.mjs', 'src/blob-io.mjs', 'src/blob-quota.mjs', 'src/transfer-blobs.mjs', 'generated/wasm/admission.js']
      .map(path => ({ type: 'ESModule', path: join(root, path) }))
      .concat({ type: 'CompiledWasm', path: join(root, 'generated/wasm/admission_bg.wasm') }),
    compatibilityDate: '2026-10-18',
    bindings: { RELAY_NAME: 'zoen-cloudflare-private', DIRECTORY_CELL: 'private-native-fixture' },
    durableObjects: { DIRECTORY: { className: 'DirectoryState', useSQLite: true } },
    ...(path ? { resourcePersistencePath: path } : {}),
  }));
}
async function call(mf, path, input = {}) {
  const response = await mf.dispatchFetch(`http://directory.test${path}`, { method: 'POST', body: JSON.stringify(input) });
  return { status: response.status, body: await response.json() };
}
const readReply = encoded => JSON.parse(directoryReadReply(Buffer.from(encoded, 'base64')));
const keyPackages = encoded => JSON.parse(directoryKeyPackageReply(Buffer.from(encoded, 'base64')))
  .sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
async function apply(mf, step, inspect = true) {
  const before = inspect ? (await call(mf, '/inspect')).body.result : null;
  const result = await call(mf, '/step', { ...step, enforce_single_clock: true });
  if (step.expected.kind === 'rejected') {
    assert.equal(result.status, 400, step.label);
    assert.equal(result.body.error.reason, step.expected.reason, step.label);
  } else {
    assert.equal(result.status, 200, JSON.stringify({ label: step.label, ...result.body }));
    if (step.unordered_reply) {
      assert.deepEqual(keyPackages(result.body.result.reply_base64), keyPackages(step.expected.reply_base64), step.label);
    } else if (step.unordered_profiles) {
      const normalize = encoded => ({ ...readReply(encoded), profiles: readReply(encoded).profiles.sort((a, b) => a.id.localeCompare(b.id)) });
      assert.deepEqual(normalize(result.body.result.reply_base64), normalize(step.expected.reply_base64), step.label);
    } else assert.deepEqual(result.body.result, step.expected, step.label);
  }
  if (inspect && step.label.startsWith('read-') && !step.label.endsWith('-register') && !step.label.startsWith('read-link-')
    && !step.label.startsWith('read-quota-publish') && !['read-unlink', 'read-claim-legacy'].includes(step.label))
    assert.deepEqual((await call(mf, '/inspect')).body.result, before, 'Directory reads must not mutate accounts, devices, boxes or packages');
  return result;
}

test('Directory reads match actual native PostgreSQL lookup limits, literal prefixes, first500 IDs, revocation and own-device inventory across restart', async () => {
  const path = await mkdtemp(join(tmpdir(), 'zoen-directory-reads-'));
  let mf = runtime(path);
  try {
    for (const step of native.steps) await apply(mf, step, false);
    for (const step of native.reads.steps) {
      const result = await apply(mf, step);
      if (step.label === 'read-unlink') {
        await mf.dispose(); mf = runtime(path);
      }
      if (step.label === 'read-prefix-limited') assert.equal(readReply(result.body.result.reply_base64).profiles.length, 20);
      if (step.label === 'read-profiles-first500') assert.equal(readReply(result.body.result.reply_base64).profiles.length, 1);
      if (step.label === 'read-devices') assert.deepEqual(readReply(result.body.result.reply_base64).devices.map(row => row.revoked), [false, true, false]);
    }
  } finally { await mf.dispose(); await rm(path, { recursive: true, force: true }); }
});

test('invalid key-package claims spend the shared account budget before validation without consuming stock or receipts', async () => {
  const mf = runtime();
  const step = label => {
    const value = native.reads.steps.find(step => step.label === label);
    assert.ok(value, label);
    return value;
  };
  const send = (label, actor = 'read-owner', now = '1700000000000') => call(mf, '/step', {
    ...step(label), actor, now_ms: now, enforce_single_clock: true,
  });
  const state = async () => (await call(mf, '/inspect')).body.result;
  try {
    for (const label of ['read-owner-login', 'read-owner-register', 'read-quota-publish', 'read-quota-publish-second', 'read-link-0', 'read-linked-login'])
      await apply(mf, step(label), false);
    const before = await state();
    assert.equal(before.packages.length, 2, 'Genuine native MLS publication stock required');
    assert.equal((await send('read-link-0')).body.error.reason, 'a link box is already there');
    assert.equal((await send('read-empty-handle')).status, 200);
    for (let index = 0; index < 57; index++) {
      const label = ['read-claim-invalid-operation', 'read-claim-too-many', 'read-claim-invalid-target'][index % 3];
      const result = await send(label);
      assert.equal(result.status, 400);
      assert.equal(result.body.error.reason, step(label).expected.reason);
    }
    assert.deepEqual(await state(), before, 'Rejected claims cannot alter directory, inventory or receipts');
    const refused = await send('read-claim-legacy');
    assert.equal(refused.status, 400);
    assert.equal(refused.body.error.reason, 'slow down, retry in 1.0 s');
    assert.equal((await send('read-empty-handle', 'read-linked')).body.error.reason, 'slow down, retry in 1.0 s');
    await apply(mf, step('read-unlink'), false);
    assert.equal((await send('read-claim-invalid-operation', 'read-linked')).body.error.reason, 'this device was unlinked', 'Current device authorization must precede quota');
    assert.equal((await send('read-claim-legacy', 'read-owner', '1700000001000')).status, 200);
    assert.equal((await state()).packages.length, 1, 'An admitted legacy claim consumes exactly one package');
  } finally { await mf.dispose(); }
});

test('account lookup quota survives restart, shares all devices and link failures, and refills against a nondecreasing clock', async () => {
  const path = await mkdtemp(join(tmpdir(), 'zoen-directory-read-quota-'));
  let mf = runtime(path);
  const step = label => {
    const value = native.reads.steps.find(step => step.label === label);
    assert.ok(value, label);
    return value;
  };
  const send = (value, actor = 'read-owner', now = '1700000000000') => call(mf, '/step', {
    ...value, actor, now_ms: now, enforce_single_clock: true,
  });
  const lookup = () => send(step('read-empty-handle'));
  try {
    await apply(mf, step('read-owner-login'), false);
    await apply(mf, step('read-owner-register'), false);
    await apply(mf, step('read-link-0'), false); // One unit, shared with Lookup.
    const failedLink = await send(step('read-link-0'));
    assert.equal(failedLink.status, 400);
    assert.equal(failedLink.body.error.reason, 'a link box is already there'); // Another unit remains spent.
    for (let index = 0; index < 58; index++) assert.equal((await lookup()).status, 200);
    await mf.dispose(); mf = runtime(path);
    assert.equal((await lookup()).body.error.reason, 'slow down, retry in 1.0 s');
    await apply(mf, step('read-linked-login'), false);
    assert.equal((await send(step('read-empty-handle'), 'read-linked')).body.error.reason, 'slow down, retry in 1.0 s');
    await apply(mf, step('read-person-0-login'), false);
    await apply(mf, step('read-person-0-register'), false);
    assert.equal((await send(step('read-empty-handle'), 'read-person-0')).status, 200);
    assert.equal((await send(step('read-empty-handle'), 'read-owner', '1700000001000')).status, 200);
    assert.equal((await lookup()).body.error.reason, 'slow down, retry in 1.0 s', 'A backward clock cannot refill the durable bucket');
  } finally { await mf.dispose(); await rm(path, { recursive: true, force: true }); }
});

test('oversized profile and device reads refuse atomically within a private byte budget instead of returning truncated success', async () => {
  const mf = runtime();
  const step = label => {
    const value = native.reads.steps.find(step => step.label === label);
    assert.ok(value, label);
    return value;
  };
  const send = label => call(mf, '/step', { ...step(label), enforce_single_clock: true });
  const capacityFailure = result => {
    assert.equal(result.status, 400);
    assert.equal(result.body.error.code, 'private_directory_read_capacity');
    assert.equal(result.body.error.permanent, false);
    assert.equal(result.body.result, undefined, 'No partial reply may escape');
  };
  try {
    for (const label of ['read-owner-login', 'read-owner-register', 'read-person-2-login', 'read-person-2-register'])
      await apply(mf, step(label), false);
    const identities = ['read-owner-register', 'read-person-2-register'].map(label => {
      const state = step(label).state_after.identities;
      return state.find(profile => profile.handle === (label === 'read-owner-register' ? 'read_owner' : 'read_02')).id;
    });
    // Each optional glyph fits a native frame; the aggregate tests a private
    // storage fault bound, not native acceptance of these fabricated payloads.
    assert.equal((await call(mf, '/read-capacity', { kind: 'profiles', identities, bytes: 400 * 1024 })).status, 200);
    capacityFailure(await send('read-prefix-limited'));
    capacityFailure(await send('read-profiles-mixed'));
    const single = await send('read-exact-normalized');
    assert.equal(single.status, 200);
    assert.equal(readReply(single.body.result.reply_base64).profiles[0].glyph.length, 400 * 1024);
    await call(mf, '/read-capacity', { kind: 'profiles', identities, bytes: 0 });
    const seeded = await call(mf, '/read-capacity', { kind: 'devices', identity: identities[0], count: 6000 });
    assert.equal(seeded.status, 200);
    assert.equal(seeded.body.result.devices, 6002);
    capacityFailure(await send('read-devices'));
  } finally { await mf.dispose(); }
});
