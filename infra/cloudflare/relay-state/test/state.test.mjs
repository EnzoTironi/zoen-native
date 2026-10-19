import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { Miniflare, convertV4MiniflareOptions } from 'miniflare';

const fixture = fileURLToPath(new URL('./fixture.mjs', import.meta.url));
const source = fileURLToPath(new URL('../src/index.mjs', import.meta.url));
const moduleRoot = fileURLToPath(new URL('../', import.meta.url));
const storage = fileURLToPath(new URL('../src/space-storage.mjs', import.meta.url));
const zero = '0'.repeat(64);
const author = 'a'.repeat(64);
function proposal(clientId, seq = '0', previous = null, bytes = [10, 20, 30]) {
  return {
    spaceId: 'space-storage-contract', author, clientId,
    expectedHead: previous,
    admissionState: [Number(BigInt(seq) + 1n)],
    event: { seq, prev: previous?.hash ?? zero, hash: (BigInt(seq) + 1n).toString(16).padStart(64, '0'), bytes },
  };
}
function runtime(path, entryScript = fixture) {
  return new Miniflare(convertV4MiniflareOptions({
    modules: [...new Set([entryScript, source, storage])].map(path => ({ type: 'ESModule', path })),
    modulesRoot: moduleRoot,
    compatibilityDate: '2026-10-18',
    durableObjects: { SPACE_LOG: { className: 'SpaceLog', useSQLite: true } },
    ...(path ? { resourcePersistencePath: path } : {}),
  }));
}
async function call(mf, input, path = '/append') {
  const response = await mf.dispatchFetch(`http://storage.test${path}`, { method: 'POST', body: JSON.stringify(input) });
  return { status: response.status, body: await response.json() };
}

test('transactional head fence, author-scoped replay and durable exact bytes survive restart', async () => {
  const path = await mkdtemp(join(tmpdir(), 'zoen-cf-state-'));
  let mf = runtime(path);
  try {
    const initial = proposal('first');
    assert.equal((await call(mf, initial)).body.kind, 'new');
    const head = { seq: '0', hash: initial.event.hash };
    const [left, right] = await Promise.all([
      call(mf, proposal('left', '1', head, [40, 50])),
      call(mf, proposal('right', '1', head, [60, 70])),
    ]);
    assert.deepEqual([left.body.kind, right.body.kind].sort(), ['conflict', 'new']);
    const replay = await call(mf, proposal('first', '0', null, [99]));
    assert.equal(replay.body.kind, 'duplicate');
    assert.deepEqual(Object.values(replay.body.event.bytes), [10, 20, 30]);
    const stateBeforeRestart = (await call(mf, { spaceId: initial.spaceId }, '/snapshot')).body;
    assert.equal(stateBeforeRestart.head.seq, '1');
    assert.deepEqual(Object.values(stateBeforeRestart.admissionState), [2]);
    await mf.dispose();
    mf = runtime(path);
    const history = await call(mf, { spaceId: initial.spaceId, from: '0', limit: 256 }, '/read');
    assert.deepEqual(history.body.map(item => item.seq), ['0', '1']);
    assert.deepEqual(Object.values(history.body[0].bytes), [10, 20, 30]);
    assert.deepEqual((await call(mf, { spaceId: initial.spaceId }, '/snapshot')).body, stateBeforeRestart);
    const duplicateAfterRestart = await call(mf, initial);
    assert.equal(duplicateAfterRestart.body.kind, 'duplicate');
    const anotherAuthor = proposal('first', '2', { seq: '1', hash: history.body[1].hash });
    anotherAuthor.author = 'b'.repeat(64);
    assert.equal((await call(mf, anotherAuthor)).body.kind, 'new');
    assert.equal((await call(mf, { spaceId: initial.spaceId, from: '1', limit: 1 }, '/read')).body.length, 1);
  } finally { await mf.dispose(); await rm(path, { recursive: true, force: true }); }
});

test('UInt64 cursors above 2^53 retain exact ordering and overflow cannot mutate state', async () => {
  const mf = runtime(false);
  try {
    const head = (await call(mf, { spaceId: 'large-sequence', seq: '9007199254740992' }, '/seed-high')).body;
    const next = proposal('next', '9007199254740993', head); next.spaceId = 'large-sequence';
    assert.equal((await call(mf, next)).body.kind, 'new');
    const history = await call(mf, { spaceId: next.spaceId, from: head.seq, limit: 2 }, '/read');
    assert.deepEqual(history.body.map(item => item.seq), ['9007199254740992', '9007199254740993']);
    const maximum = (await call(mf, { spaceId: 'max-sequence', seq: '18446744073709551615' }, '/seed-high')).body;
    const overflow = proposal('overflow', '0', maximum); overflow.spaceId = 'max-sequence';
    assert.equal((await call(mf, overflow)).status, 400);
    assert.deepEqual((await call(mf, { spaceId: overflow.spaceId }, '/snapshot')).body.head, maximum);
  } finally { await mf.dispose(); }
});

test('production HTTP entry exposes no sync, authentication, readiness or storage write route', async () => {
  const mf = runtime(false, source);
  try {
    for (const path of ['/healthz', '/readyz', '/v1/sync', '/v1/auth/challenge', '/append']) {
      assert.equal((await mf.dispatchFetch(`http://storage.test${path}`, { method: 'POST', body: '{}' })).status, 404);
    }
  } finally { await mf.dispose(); }
});

test('invalid sequences, hashes, oversized entries and unbounded reads cannot change the log', async () => {
  const mf = runtime(false);
  try {
    for (const seq of ['-1', '01', '1e3', '18446744073709551616', 0]) {
      const input = proposal('invalid'); input.event.seq = seq;
      assert.equal((await call(mf, input)).status, 400);
    }
    const skipped = proposal('skipped', '9007199254740993');
    assert.equal((await call(mf, skipped)).status, 400);
    const wrongHash = proposal('hash'); wrongHash.event.hash = '*';
    assert.equal((await call(mf, wrongHash)).status, 400);
    const tooLarge = proposal('large', '0', null, Array(1024 * 1024 + 1).fill(1));
    assert.equal((await call(mf, tooLarge)).status, 400);
    assert.equal((await call(mf, { spaceId: skipped.spaceId, from: '0', limit: 257 }, '/read')).status, 400);
    assert.deepEqual((await call(mf, { spaceId: skipped.spaceId, from: '0', limit: 1 }, '/read')).body, []);
    assert.equal((await call(mf, proposal('valid'))).body.kind, 'new');
  } finally { await mf.dispose(); }
});
