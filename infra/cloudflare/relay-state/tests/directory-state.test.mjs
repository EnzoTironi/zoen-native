import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { Miniflare, convertV4MiniflareOptions } from 'miniflare';

const moduleRoot = fileURLToPath(new URL('../', import.meta.url));
const native = JSON.parse(await readFile(process.env.ZOEN_DIRECTORY_FIXTURE
  ? resolve(process.env.ZOEN_DIRECTORY_FIXTURE) : join(moduleRoot, 'generated/directory-fixture.json'), 'utf8'));
assert.equal(native.schema, 1);
assert.equal(native.steps.length, 33, 'Every isolated native PostgreSQL scenario is required');
function runtime(path, production = false) {
  return new Miniflare(convertV4MiniflareOptions({
    modulesRoot: moduleRoot,
    modules: [production ? 'src/directory-index.mjs' : 'tests/directory-fixture.mjs',
      'src/directory-state.mjs', 'generated/wasm/admission.js']
      .map(path => ({ type: 'ESModule', path: join(moduleRoot, path) }))
      .concat({ type: 'CompiledWasm', path: join(moduleRoot, 'generated/wasm/admission_bg.wasm') }),
    compatibilityDate: '2026-10-18',
    bindings: { RELAY_NAME: 'zoen-cloudflare-private', DIRECTORY_CELL: 'private-native-fixture' },
    durableObjects: { DIRECTORY: { className: 'DirectoryState', useSQLite: true } },
    ...(path ? { resourcePersistencePath: path } : {}),
  }));
}
async function call(mf, path, input = {}) {
  const response = await mf.dispatchFetch(`http://directory-private.test${path}`, { method: 'POST', body: JSON.stringify(input) });
  return { status: response.status, body: await response.json() };
}
async function inspect(mf) { return (await call(mf, '/inspect')).body.result; }
async function apply(mf, step) {
  const before = await inspect(mf);
  const response = await call(mf, '/step', step);
  if (step.expected.kind === 'rejected') {
    assert.equal(response.status, 400, JSON.stringify(response.body));
    assert.equal(response.body.error.reason, step.expected.reason);
    assert.deepEqual(await inspect(mf), before, 'Rejection cannot mutate identities, devices, or boxes');
  } else {
    assert.equal(response.status, 200, JSON.stringify(response.body));
    assert.deepEqual(response.body.result, step.expected);
  }
  assert.deepEqual(await inspect(mf), step.state_after, step.label);
  return response;
}

test('private Directory matches native PostgreSQL enrollment, sponsorship, uniqueness, revocation and one-time boxes across restart', async () => {
  const path = await mkdtemp(join(tmpdir(), 'zoen-directory-'));
  let mf = runtime(path);
  try {
    let restarted = false;
    for (const step of native.steps) {
      await apply(mf, step);
      if (!restarted && step.label === 'deliver-first') {
        const before = await inspect(mf);
        await mf.dispose(); mf = runtime(path);
        assert.deepEqual(await inspect(mf), before);
        restarted = true;
      }
    }
    assert.ok(restarted);
  } finally { await mf.dispose(); await rm(path, { recursive: true, force: true }); }
});

for (const [label, table] of [['register-first', 'device'], ['deliver-first', 'box']]) {
  test(`failed late ${label} write rolls back identity, enrollment, box and session transition before exact retry`, async () => {
    const mf = runtime();
    try {
      const index = native.steps.findIndex(step => step.label === label);
      assert.ok(index >= 0);
      for (const step of native.steps.slice(0, index)) await apply(mf, step);
      const before = await inspect(mf);
      const sessions = (await call(mf, '/sessions')).body.result;
      assert.equal((await call(mf, '/reject', { table })).status, 200);
      assert.equal((await call(mf, '/step', native.steps[index])).status, 400);
      assert.deepEqual(await inspect(mf), before);
      assert.deepEqual((await call(mf, '/sessions')).body.result, sessions);
      assert.equal((await call(mf, '/allow')).status, 200);
      await apply(mf, native.steps[index]);
    } finally { await mf.dispose(); }
  });
}

test('simultaneous preopened first-registration sessions produce one atomic registration and one refusal', async () => {
  const mf = runtime();
  try {
    const index = native.steps.findIndex(step => step.label === 'register-first');
    for (const step of native.steps.slice(0, index)) await apply(mf, step);
    const original = native.steps[index];
    // The oracle explicitly logs in a second provisional session for this identity.
    const stale = native.steps.find(step => step.label === 'register-stale');
    assert.ok(stale, 'Native fixture must include a preopened provisional session');
    const results = await Promise.all([call(mf, '/step', original), call(mf, '/step', stale)]);
    assert.deepEqual(results.map(result => result.status).sort(), [200, 400]);
    assert.equal(results.find(result => result.status === 400).body.error.reason, 'account already registered; link this device first');
    assert.deepEqual(await inspect(mf), original.state_after);
  } finally { await mf.dispose(); }
});

test('sponsor revocation and concurrent enrollment retry serialize without reviving the device', async () => {
  for (const revokeFirst of [false, true]) {
    const mf = runtime();
    try {
      const index = native.steps.findIndex(step => step.label === 'unlink-sponsored-target');
      assert.ok(index >= 0);
      for (const step of native.steps.slice(0, index)) await apply(mf, step);
      const unlink = native.steps[index];
      const original = native.steps.find(step => step.label === 'deliver-first');
      const retry = { ...original, actor: 'linked' };
      let revocation;
      let delivery;
      if (revokeFirst) {
        revocation = await call(mf, '/step', unlink);
        delivery = await call(mf, '/step', retry);
      } else {
        [revocation, delivery] = await Promise.all([call(mf, '/step', unlink), call(mf, '/step', retry)]);
      }
      assert.equal(revocation.status, 200); assert.deepEqual(revocation.body.result, unlink.expected);
      const expected = structuredClone(unlink.state_after);
      if (delivery.status === 200) {
        assert.equal(revokeFirst, false, 'Fresh revoked sponsor cannot enroll or publish a link box');
        assert.deepEqual(delivery.body.result, original.expected);
        expected.boxes = structuredClone(original.state_after.boxes);
      } else {
        assert.equal(delivery.status, 400); assert.equal(delivery.body.error.reason, 'this device was unlinked');
      }
      assert.deepEqual(await inspect(mf), expected, 'Only a serialized admitted retry may leave the original sealed box');
    } finally { await mf.dispose(); }
  }
});

test('failed, consumed, mismatched and expired challenges cannot establish reusable sessions', async () => {
  const mf = runtime();
  try {
    const login = native.steps.find(step => step.op === 'login' && step.expected.kind === 'session');
    for (const invalid of [{ auth_base64: '' }, { auth_base64: Buffer.from([0]).toString('base64') },
      { auth_base64: Buffer.alloc(1024 * 1024 + 1).toString('base64') }, { frame_null: true }]) {
      const auth = await call(mf, '/start', login);
      assert.equal((await call(mf, '/finish', { ...login, ...invalid, challenge_id: auth.body.result.challenge_id })).status, 400);
      const retry = await call(mf, '/finish', { ...login, challenge_id: auth.body.result.challenge_id });
      assert.equal(retry.status, 400); assert.equal(retry.body.error.reason, 'private challenge unavailable');
    }
    const other = native.steps.find(step => step.op === 'login' && step.expected.device === login.expected.device
      && step.auth_base64 !== login.auth_base64);
    assert.ok(other, 'Two native signatures for distinct challenges are required');
    const bound = (await call(mf, '/start', login)).body.result;
    const mismatch = await call(mf, '/finish', { ...login, auth_base64: other.auth_base64, challenge_id: bound.challenge_id });
    assert.equal(mismatch.status, 400); assert.equal(mismatch.body.error.reason, 'login signature invalid');
    assert.equal((await call(mf, '/finish', { ...login, challenge_id: bound.challenge_id })).body.error.reason,
      'private challenge unavailable');
    const expired = (await call(mf, '/start', login)).body.result;
    await call(mf, '/clock', { now_ms: '1700000010000' });
    const response = await call(mf, '/finish', { ...login, challenge_id: expired.challenge_id });
    assert.equal(response.status, 400); assert.equal(response.body.error.reason, 'private challenge expired');
    assert.deepEqual((await call(mf, '/sessions')).body.result, []);
  } finally { await mf.dispose(); }
});

test('session capacity refusal still consumes the authenticated challenge once', async () => {
  const mf = runtime();
  try {
    const login = native.steps.find(step => step.op === 'login' && step.expected.kind === 'session');
    const challenge = (await call(mf, '/start', login)).body.result;
    assert.equal((await call(mf, '/fill-sessions')).status, 200);
    const result = await call(mf, '/finish', { ...login, challenge_id: challenge.challenge_id });
    assert.equal(result.status, 400); assert.equal(result.body.error.reason, 'private directory session capacity reached');
    const replay = await call(mf, '/finish', { ...login, challenge_id: challenge.challenge_id });
    assert.equal(replay.status, 400); assert.equal(replay.body.error.reason, 'private challenge unavailable');
  } finally { await mf.dispose(); }
});

test('one transaction clock governs last-fresh-millisecond box lookup and consumption', async () => {
  const mf = runtime();
  try {
    const index = native.steps.findIndex(step => step.label === 'fetch-first');
    assert.ok(index >= 0, 'Native fixture must identify the first consuming fetch');
    for (const step of native.steps.slice(0, index)) await apply(mf, step);
    const step = native.steps[index];
    const result = await call(mf, '/step', { ...step, now_ms: '1700000899999', enforce_single_clock: true });
    assert.equal(result.status, 200, JSON.stringify(result.body));
    assert.deepEqual(result.body.result, step.expected);
    assert.deepEqual(await inspect(mf), step.state_after);
  } finally { await mf.dispose(); }
});

test('private production configuration has no HTTP facade or raw enrollment entrypoint', async () => {
  const mf = runtime(undefined, true);
  try {
    for (const path of ['/', '/healthz', '/sync', '/hello', '/auth', '/register', '/enroll']) {
      const response = await mf.dispatchFetch(`https://directory-private.test${path}`);
      assert.equal(response.status, 404);
    }
    const config = JSON.parse(await readFile(join(moduleRoot, 'wrangler.directory.jsonc'), 'utf8'));
    assert.equal(config.workers_dev, false); assert.equal(config.preview_urls, false); assert.deepEqual(config.routes, []);
    assert.equal(config.main, 'src/directory-index.mjs');
    assert.equal(config.account_id, '86e8a5b2686413989e8bef1f75c29079');
    assert.equal(config.durable_objects.bindings.length, 1);
  } finally { await mf.dispose(); }
});
