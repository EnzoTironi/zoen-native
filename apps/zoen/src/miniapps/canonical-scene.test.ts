import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';
import {
  CanonicalSceneHost,
  CANONICAL_SCENE_CHUNK_BYTES as CHUNK,
  MAX_CANONICAL_SCENE_BYTES as MAX,
  type CanonicalSceneDoc,
  type CanonicalScenePhase,
  type CanonicalSceneSave,
} from './canonical-scene.ts';
import type { CallToolResult } from '@modelcontextprotocol/client';

const item = 'it_scene';
const sha = (bytes: Uint8Array | string) => createHash('sha256').update(bytes).digest('hex');
const raw = '{"elements":[],"custom":{"integer":9007199254740993,"x":0.12515795230865479}}';
const doc = (scene_json = raw, version = 1, current_version = version): CanonicalSceneDoc => ({
  item_id: item,
  version,
  current_version,
  scene_json,
  sha256: sha(scene_json),
});
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((a, b) => {
    resolve = a;
    reject = b;
  });
  return { promise, resolve, reject };
}
function data(response: CallToolResult): Record<string, unknown> {
  const value = response.structuredContent!;
  assert.ok(value && typeof value === 'object' && !Array.isArray(value));
  assert.equal(response.content[0].type, 'text');
  if (response.content[0].type === 'text')
    assert.deepEqual(JSON.parse(response.content[0].text), value);
  return value as Record<string, unknown>;
}
function fixture(
  save?: (input: CanonicalSceneSave) => Promise<CanonicalSceneDoc>,
  initialDoc = doc(),
) {
  let owner = true;
  let serial = 0;
  const calls: CanonicalSceneSave[] = [];
  const phases: CanonicalScenePhase[] = [];
  const host = new CanonicalSceneHost({
    initialDoc,
    check() {
      if (!owner) throw new Error('Owner changed');
    },
    id: () => `transfer_${++serial}`,
    save: async (input) => {
      calls.push(input);
      return save ? save(input) : doc(input.scene_json, input.expected_version + 1);
    },
    onState: (phase) => phases.push(phase),
  });
  return {
    host,
    calls,
    phases,
    retire() {
      owner = false;
    },
    ids: () => serial,
  };
}
async function begin(
  host: CanonicalSceneHost,
  bytes: Uint8Array,
  mutationId = 'mutation_1',
  digest = sha(bytes),
) {
  return String(
    data(
      await host.call('app_scene_begin', {
        id: item,
        mutationId,
        bytes: bytes.length,
        sha256: digest,
      }),
    ).uploadId,
  );
}
async function append(host: CanonicalSceneHost, uploadId: string, bytes: Uint8Array) {
  for (let offset = 0, index = 0; offset < bytes.length; offset += CHUNK, index++) {
    const chunk = bytes.subarray(offset, offset + CHUNK);
    assert.equal(
      data(
        await host.call('app_scene_append', {
          id: item,
          uploadId,
          index,
          data: Buffer.from(chunk).toString('base64'),
        }),
      ).nextIndex,
      index + 1,
    );
  }
}
async function stage(host: CanonicalSceneHost, scene = raw + '\n', mutationId = 'mutation_1') {
  const bytes = Buffer.from(scene);
  const id = await begin(host, bytes, mutationId);
  await append(host, id, bytes);
  return id;
}

test('chunked reads preserve actual UTF-8 bytes, numeric tokens and a multibyte boundary beyond an event', async () => {
  const scene = ' '.repeat(CHUNK - 1) + '✏️ Café 日本語\n' + raw + ' '.repeat(CHUNK + 3);
  const f = fixture(undefined, doc(scene));
  const read = data(await f.host.call('app_scene_read', { id: item }));
  assert.equal(read.bytes, Buffer.byteLength(scene));
  assert.equal(read.chunkBytes, CHUNK);
  assert.equal(read.sha256, sha(scene));
  assert.equal(read.version, 1);
  assert.equal(read.currentVersion, 1);
  const chunks: Buffer[] = [];
  for (let index = 0; index < Math.ceil(Number(read.bytes) / CHUNK); index++) {
    const part = data(
      await f.host.call('app_scene_read_chunk', {
        id: item,
        transferId: read.transferId,
        index,
      }),
    );
    assert.equal(part.index, index);
    chunks.push(Buffer.from(String(part.data), 'base64'));
  }
  assert.deepEqual(Buffer.concat(chunks), Buffer.from(scene));
  assert.equal(data(await f.host.call('app_scene_read', { id: item })).transferId, read.transferId);
  assert.equal(f.calls.length, 0);
  f.host.dispose();
});

test('every tool is bound to the exact own Item ID and foreign transfer IDs do not read or mutate', async () => {
  const f = fixture();
  for (const name of [
    'app_scene_read',
    'app_scene_read_chunk',
    'app_scene_begin',
    'app_scene_append',
    'app_scene_commit',
    'app_scene_abort',
  ]) {
    await assert.rejects(f.host.call(name, { id: 'it_foreign' }), /current Item/);
    await assert.rejects(f.host.call(name, Object.create({ id: item })), /current Item/);
  }
  await assert.rejects(f.host.call('unknown', { id: item }), /unavailable/);
  const read = data(await f.host.call('app_scene_read', { id: item }));
  for (const args of [
    { transferId: 'foreign', index: 0 },
    { transferId: read.transferId, index: -1 },
    { transferId: read.transferId, index: 1 },
    { transferId: read.transferId, index: 0.5 },
  ])
    await assert.rejects(f.host.call('app_scene_read_chunk', { id: item, ...args }), /chunk/);
  assert.equal(f.calls.length, 0);
  f.host.dispose();
});

test('uploads require ordered complete chunks; exact repeats succeed and changed bytes cannot replace a chunk', async () => {
  const f = fixture();
  const bytes = Buffer.from('x'.repeat(CHUNK + 1));
  const id = await begin(f.host, bytes);
  const first = {
    id: item,
    uploadId: id,
    index: 0,
    data: bytes.subarray(0, CHUNK).toString('base64'),
  };
  await assert.rejects(f.host.call('app_scene_append', { ...first, index: 1 }), /size|order/);
  await assert.rejects(f.host.call('app_scene_commit', { id: item, uploadId: id }), /incomplete/);
  assert.equal(data(await f.host.call('app_scene_append', first)).nextIndex, 1);
  assert.equal(data(await f.host.call('app_scene_append', first)).nextIndex, 1);
  await assert.rejects(
    f.host.call('app_scene_append', {
      ...first,
      data: Buffer.alloc(CHUNK, 0x79).toString('base64'),
    }),
    /changed/,
  );
  await assert.rejects(
    f.host.call('app_scene_begin', {
      id: item,
      mutationId: 'other',
      bytes: 1,
      sha256: sha('y'),
    }),
    /already pending/,
  );
  await f.host.call('app_scene_append', { id: item, uploadId: id, index: 1, data: 'eA==' });
  await f.host.call('app_scene_commit', { id: item, uploadId: id });
  assert.equal(f.calls[0].scene_json, bytes.toString());
  assert.equal(data(await f.host.call('app_scene_append', first)).nextIndex, 1);
  f.host.dispose();
});

test('base64 must be canonical padded standard encoding, without ignored trailing bits or extra data', async () => {
  const f = fixture();
  const id = await begin(f.host, Buffer.from('a'));
  for (const value of [
    'YQ',
    'YR==',
    'YQ==\n',
    'YQ===',
    '-w==',
    'A===',
    'YQ==YQ==',
    ' '.repeat(4),
    'A'.repeat((CHUNK / 3) * 4 + 4),
  ])
    await assert.rejects(
      f.host.call('app_scene_append', {
        id: item,
        uploadId: id,
        index: 0,
        data: value,
      }),
      /base64|size/,
    );
  await f.host.call('app_scene_append', { id: item, uploadId: id, index: 0, data: 'YQ==' });
  await f.host.call('app_scene_commit', { id: item, uploadId: id });
  assert.equal(f.calls[0].scene_json, 'a');
  f.host.dispose();
});

test('full digest and complete strict UTF-8 are verified before any backend invocation', async () => {
  for (const bytes of [
    Buffer.from([0xed, 0xa0, 0x80]),
    Buffer.from([0xc0, 0x80]),
    Buffer.from([0xf4, 0x90, 0x80, 0x80]),
    Buffer.from([0xe2, 0x82]),
  ]) {
    const f = fixture();
    const id = await begin(f.host, bytes);
    await append(f.host, id, bytes);
    await assert.rejects(f.host.call('app_scene_commit', { id: item, uploadId: id }), /UTF-8/);
    assert.equal(f.calls.length, 0);
    assert.equal(f.host.failedScene(), null);
    f.host.dispose();
  }
  const f = fixture();
  const bytes = Buffer.from(raw);
  const id = await begin(f.host, bytes, 'bad_hash', sha('different'));
  await append(f.host, id, bytes);
  await assert.rejects(f.host.call('app_scene_commit', { id: item, uploadId: id }), /hash/);
  assert.equal(f.calls.length, 0);
  await f.host.call('app_scene_abort', { id: item, uploadId: id });
  const valid = await stage(f.host);
  await f.host.call('app_scene_commit', { id: item, uploadId: valid });
  assert.equal(f.calls.length, 1);
  f.host.dispose();
});

test('concurrent and acknowledged commit retries share one promise and cannot append another version', async () => {
  const gate = deferred<CanonicalSceneDoc>();
  const f = fixture(() => gate.promise);
  const oldRead = data(await f.host.call('app_scene_read', { id: item }));
  const scene = raw + '\n';
  const id = await stage(f.host, scene);
  const first = f.host.call('app_scene_commit', { id: item, uploadId: id });
  const same = f.host.call('app_scene_commit', { id: item, uploadId: id });
  assert.strictEqual(first, same);
  await Promise.resolve();
  assert.equal(f.calls.length, 1);
  gate.resolve(doc(scene, 2));
  assert.deepEqual(data(await first), {
    checkpointId: item,
    version: 2,
    sha256: sha(scene),
    currentVersion: 2,
  });
  assert.strictEqual(f.host.call('app_scene_commit', { id: item, uploadId: id }), first);
  await assert.rejects(
    f.host.call('app_scene_read_chunk', { id: item, transferId: oldRead.transferId, index: 0 }),
    /chunk/,
  );
  const current = data(await f.host.call('app_scene_read', { id: item }));
  assert.equal(current.version, 2);
  assert.equal(current.sha256, sha(scene));
  const next = await stage(f.host, scene + ' ', 'mutation_2');
  assert.notEqual(next, id);
  assert.strictEqual(f.host.call('app_scene_commit', { id: item, uploadId: id }), first);
  assert.equal(f.calls.length, 1);
  await f.host.call('app_scene_abort', { id: item, uploadId: next });
  f.host.dispose();
});

test('lost ACK retries use the same CAS input and accepted historical version never adopts a newer peer base', async () => {
  const scene = raw + '\n';
  let attempts = 0;
  const f = fixture(async () => {
    if (++attempts === 1) throw new Error('ACK lost after persistence');
    if (attempts === 2) return doc(scene, 2, 5);
    throw new Error('Stale: peer version 5');
  });
  const id = await stage(f.host, scene);
  assert.equal(f.host.failedScene(), null);
  await assert.rejects(f.host.call('app_scene_commit', { id: item, uploadId: id }), /ACK lost/);
  assert.deepEqual(f.host.failedScene(), { scene_json: scene, mutation_id: 'mutation_1' });
  assert.equal(data(await f.host.call('app_scene_read', { id: item })).version, 1);
  const accepted = data(await f.host.call('app_scene_commit', { id: item, uploadId: id }));
  assert.strictEqual(f.calls[0], f.calls[1]);
  assert.equal(accepted.currentVersion, 5);
  assert.equal(accepted.version, 2);
  assert.deepEqual(f.host.failedScene(), { scene_json: scene, mutation_id: 'mutation_1' });
  assert.equal(f.phases.at(-1), 'error');
  assert.equal(data(await f.host.call('app_scene_commit', { id: item, uploadId: id })).version, 2);
  assert.equal(f.calls.length, 2, 'a durable historical ACK cannot cause a repeated write');
  await assert.rejects(begin(f.host, Buffer.from(scene + ' '), 'mutation_2'), /already pending/);
  await f.host.call('app_scene_abort', { id: item, uploadId: id });
  const id2 = await stage(f.host, scene + ' ', 'mutation_2');
  await assert.rejects(f.host.call('app_scene_commit', { id: item, uploadId: id2 }), /Stale/);
  assert.equal(f.calls[2].expected_version, 2);
  assert.equal(f.calls[2].expected_sha256, sha(scene));
  assert.equal(data(await f.host.call('app_scene_read', { id: item })).version, 2);
  assert.equal(f.host.failedScene()?.scene_json, scene + ' ');
  f.host.dispose();
});

test('native copy preparation verifies the newest complete staged bytes without writing or exposing a guest copy tool', async () => {
  const f = fixture();
  const scene = raw + ' '.repeat(CHUNK) + '✏️';
  const bytes = Buffer.from(scene);
  const id = await begin(f.host, bytes, 'copy_mutation');
  assert.throws(() => f.host.stagedScene(id, bytes.length, sha(bytes)), /incomplete/);
  await append(f.host, id, bytes);
  for (const [uploadId, size, digest] of [
    ['other', bytes.length, sha(bytes)],
    [id, bytes.length - 1, sha(bytes)],
    [id, bytes.length, sha('wrong')],
  ] as const)
    assert.throws(() => f.host.stagedScene(uploadId, size, digest), /match/);
  const staged = f.host.stagedScene(id, bytes.length, sha(bytes));
  assert.deepEqual(staged, { scene_json: scene, mutation_id: 'copy_mutation' });
  assert.ok(Object.isFrozen(staged));
  assert.equal(f.calls.length, 0);
  assert.equal(data(await f.host.call('app_scene_read', { id: item })).version, 1);
  assert.equal(f.host.failedScene(), null);
  await assert.rejects(f.host.call('stagedScene', { id: item, uploadId: id }), /unavailable/);
  await f.host.call('app_scene_abort', { id: item, uploadId: id });
  assert.throws(() => f.host.stagedScene(id, bytes.length, sha(bytes)), /match/);
  f.host.dispose();
  assert.throws(() => f.host.stagedScene(id, bytes.length, sha(bytes)), /closed/);
});

test('a whole multi-chunk wire retry repeats original per-index ACKs before retrying the exact commit', async () => {
  const scene = raw + ' '.repeat(CHUNK * 2);
  let attempts = 0;
  const f = fixture(async () => {
    if (++attempts === 1) throw new Error('ACK lost');
    return doc(scene, 2);
  });
  const bytes = Buffer.from(scene);
  const first = await stage(f.host, scene);
  await assert.rejects(f.host.call('app_scene_commit', { id: item, uploadId: first }), /ACK lost/);
  for (const expectedCalls of [2, 2]) {
    const retryId = await begin(f.host, bytes);
    assert.equal(retryId, first);
    await append(f.host, retryId, bytes);
    assert.equal(
      data(await f.host.call('app_scene_commit', { id: item, uploadId: retryId })).version,
      2,
    );
    assert.equal(f.calls.length, expectedCalls);
  }
  assert.strictEqual(f.calls[0], f.calls[1]);
  f.host.dispose();
});

test('an unrelated newer DTO or incoherent ACK never replaces accepted state or drops failed scene bytes', async () => {
  const scene = raw + '\n';
  for (const response of [
    doc('unrelated', 3),
    { ...doc(scene, 2), item_id: 'it_foreign' },
    { ...doc(scene, 2), sha256: sha('wrong') },
    doc(scene, 1),
    { ...doc(scene, 2), current_version: 1 },
  ]) {
    const f = fixture(async () => response);
    const id = await stage(f.host, scene);
    await assert.rejects(
      f.host.call('app_scene_commit', { id: item, uploadId: id }),
      /acknowledge|Invalid/,
    );
    assert.equal(data(await f.host.call('app_scene_read', { id: item })).version, 1);
    assert.equal(f.host.failedScene()?.scene_json, scene);
    await f.host.call('app_scene_abort', { id: item, uploadId: id });
    assert.equal(f.host.failedScene(), null);
    f.host.dispose();
  }
});

test('abort drains an admitted commit without reopening setters or undoing its canonical acceptance', async () => {
  const gate = deferred<CanonicalSceneDoc>();
  const f = fixture(() => gate.promise);
  const scene = raw + '\n';
  const id = await stage(f.host, scene);
  const commit = f.host.call('app_scene_commit', { id: item, uploadId: id });
  await Promise.resolve();
  const abort = f.host.call('app_scene_abort', { id: item, uploadId: id });
  await assert.rejects(begin(f.host, Buffer.from('b'), 'mutation_2'), /already pending|aborted/);
  gate.resolve(doc(scene, 2));
  await commit;
  await abort;
  assert.equal(data(await f.host.call('app_scene_read', { id: item })).version, 2);
  await f.host.call('app_scene_abort', { id: item, uploadId: id });
  const next = await stage(f.host, scene + ' ', 'mutation_2');
  assert.notEqual(next, id);
  assert.equal(f.calls.length, 1);
  f.host.dispose();
});

test('abort before commit admission synchronously removes its authority, including an immediate stale commit callback', async () => {
  const f = fixture();
  const id = await stage(f.host);
  const aborted = f.host.call('app_scene_abort', { id: item, uploadId: id });
  const stale = f.host.call('app_scene_commit', { id: item, uploadId: id });
  await aborted;
  await assert.rejects(stale, /Invalid scene upload/);
  assert.equal(f.calls.length, 0);
  assert.equal(f.host.failedScene(), null);
  const next = await stage(f.host, raw + ' ', 'next');
  await f.host.call('app_scene_abort', { id: item, uploadId: id });
  await f.host.call('app_scene_commit', { id: item, uploadId: next });
  assert.equal(f.calls.length, 1, 'an old abort cannot clear the newer upload');
  f.host.dispose();
});

test('dispose before commit admission prevents save; disposal after admission rejects late completion and callbacks', async () => {
  const first = fixture();
  const id = await stage(first.host);
  const never = first.host.call('app_scene_commit', { id: item, uploadId: id });
  first.host.dispose();
  await assert.rejects(never, /closed/);
  assert.equal(first.calls.length, 0);
  assert.deepEqual(first.phases, ['closed']);
  assert.equal(first.host.failedScene(), null);
  const gate = deferred<CanonicalSceneDoc>();
  const f = fixture(() => gate.promise);
  const next = await stage(f.host);
  const pending = f.host.call('app_scene_commit', { id: item, uploadId: next });
  await Promise.resolve();
  f.host.dispose();
  f.host.dispose();
  gate.resolve(doc(raw + '\n', 2));
  await assert.rejects(pending, /closed/);
  assert.deepEqual(f.phases, ['saving', 'closed']);
  assert.equal(f.calls.length, 1);
  await assert.rejects(f.host.call('app_scene_read', { id: item }), /closed/);
});

test('owner change during save or before failed-scene access permanently retires the old capability', async () => {
  const gate = deferred<CanonicalSceneDoc>();
  const f = fixture(() => gate.promise);
  const id = await stage(f.host);
  const pending = f.host.call('app_scene_commit', { id: item, uploadId: id });
  await Promise.resolve();
  f.retire();
  gate.resolve(doc(raw + '\n', 2));
  await assert.rejects(pending, /Owner changed/);
  assert.deepEqual(f.phases, ['saving']);
  assert.equal(f.host.failedScene(), null);
  await assert.rejects(f.host.call('app_scene_read', { id: item }), /closed/);
  const failed = fixture(async () => {
    throw new Error('Stale');
  });
  const failedId = await stage(failed.host);
  await assert.rejects(
    failed.host.call('app_scene_commit', { id: item, uploadId: failedId }),
    /Stale/,
  );
  failed.retire();
  assert.throws(() => failed.host.failedScene(), /Owner changed/);
  assert.equal(failed.host.failedScene(), null);
});

test('allocation limits and initial document integrity fail closed before transfer IDs or saves', async () => {
  const f = fixture();
  for (const bytes of [MAX + 1, 2 ** 32, Number.MAX_SAFE_INTEGER, 0, -1, 0.5, Infinity])
    await assert.rejects(
      f.host.call('app_scene_begin', { id: item, mutationId: 'm', bytes, sha256: sha('a') }),
      /oversized/,
    );
  for (const mutationId of ['', 'x'.repeat(129), 'has space', '../foreign'])
    await assert.rejects(
      f.host.call('app_scene_begin', { id: item, mutationId, bytes: 1, sha256: sha('a') }),
      /Invalid/,
    );
  assert.equal(f.ids(), 0);
  assert.equal(f.calls.length, 0);
  for (const invalid of [
    { ...doc(), sha256: sha('wrong') },
    { ...doc(), current_version: 0 },
    { ...doc(), version: 0 },
    { ...doc(), current_version: 2 ** 32 },
    doc('\ud800'),
    doc('é'.repeat(MAX / 2 + 1)),
  ])
    assert.throws(() => fixture(undefined, invalid), /scene|UTF-8|limit/i);
  f.host.dispose();
});
