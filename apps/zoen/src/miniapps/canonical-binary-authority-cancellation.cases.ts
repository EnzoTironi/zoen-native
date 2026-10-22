import assert from 'node:assert/strict';
import test from 'node:test';
import type { AppBinaryDocumentDto } from '../core/generated/AppBinaryDocumentDto';
import {
  changed,
  deferred,
  dto,
  enginePort,
  fixture,
  intent,
  original,
} from './canonical-binary.fixtures.ts';
import { CANONICAL_SCENE_CHUNK_BYTES as CHUNK, encode64, hash } from './canonical-scene.ts';
import { attachDeckcraftDocument, DeckcraftDocument } from './deckcraft-document.ts';

test('owner retirement while saving cannot accept into a retired engine or publish into a replacement binding', async (t) => {
  const started = deferred<void>();
  const release = deferred<AppBinaryDocumentDto>();
  const f = await fixture({
    save: async () => {
      started.resolve();
      return release.promise;
    },
  });
  t.after(f.dispose);
  const engine = enginePort();
  const document = new DeckcraftDocument({
    app: f.app,
    engine,
    itemId: 'it_deck',
    id: () => 'mutation',
  });
  await document.open();
  engine.type(changed);
  const saving = document.save();
  const rejected = assert.rejects(saving);
  await started.promise;
  f.retire();
  document.retireAuthority();
  release.resolve(dto(changed, 2));
  await rejected;
  assert.equal(engine.accepts(), 0);
  await assert.rejects(document.save(), /closed/);
});
test('wrong artifact, spoofed author, reordered/mutated chunks and mismatched base cannot escape this binding', async (t) => {
  const f = await fixture();
  t.after(f.dispose);
  await assert.rejects(f.tool('app_binary_read', { id: 'other' }));
  await assert.rejects(f.tool('app_binary_read', { actorId: 'spoof' }));
  const args = {
    mutationId: 'm',
    version: 1,
    sha256: hash(original),
    bytes: changed.length,
    documentSha256: hash(changed),
    intentSha256: intent,
  };
  const begin = await f.tool('app_binary_begin', args);
  await assert.rejects(f.tool('app_binary_begin', { ...args, version: 2 }));
  await assert.rejects(
    f.tool('app_binary_append', { uploadId: begin.uploadId, index: 1, data: encode64(changed) }),
  );
  await f.tool('app_binary_append', {
    uploadId: begin.uploadId,
    index: 0,
    data: encode64(changed),
  });
  const modified = changed.slice();
  modified[0] ^= 1;
  await assert.rejects(
    f.tool('app_binary_append', { uploadId: begin.uploadId, index: 0, data: encode64(modified) }),
  );
  assert.equal(f.saves.length, 0);
});
test('a late read cannot replace a newer exact export source and selected hash is mandatory', async (t) => {
  const old = deferred<AppBinaryDocumentDto>();
  let reads = 0;
  const f = await fixture({ read: async () => (++reads === 1 ? old.promise : dto(changed, 2)) });
  t.after(f.dispose);
  const first = f.binary.call('app_binary_read', { id: 'it_deck' });
  const refused = assert.rejects(first, /replaced/);
  const source = await f.binary.call('app_binary_export_source', {
    id: 'it_deck',
    version: 2,
    sha256: hash(changed),
  });
  old.resolve(dto(original));
  await refused;
  const transferId = (source.structuredContent as Record<string, unknown>).transferId;
  const chunk = await f.binary.call('app_binary_read_chunk', {
    id: 'it_deck',
    transferId,
    index: 0,
  });
  assert.equal((chunk.structuredContent as Record<string, unknown>).data, encode64(changed));
  await assert.rejects(f.tool('app_binary_export_source', { version: 2 }));
});
test('chunk tools remain below the unchanged guest message ceiling', async (t) => {
  const f = await fixture();
  t.after(f.dispose);
  const bytes = new Uint8Array(CHUNK);
  bytes.fill(77);
  const encoded = JSON.stringify({
    jsonrpc: '2.0',
    id: 1,
    method: 'tools/call',
    params: {
      name: 'app_binary_append',
      arguments: { id: 'it_deck', uploadId: 'upload', index: 0, data: encode64(bytes) },
    },
  });
  assert(encoded.length < 1_500_000);
  assert(encoded.length < 263_000);
});

test('abort distinguishes locally cancelled, committed and unknown writes without discarding a lost-response intent', async (t) => {
  let durable: AppBinaryDocumentDto | null = null;
  let attempts = 0;
  const f = await fixture({
    save: async (input) => {
      attempts++;
      if (durable) return durable;
      durable = dto(new Uint8Array(Buffer.from(input.data_base64, 'base64')), 2);
      throw new Error('synthetic lost reply after committed bytes');
    },
  });
  t.after(f.dispose);
  const args = {
    mutationId: 'unknown',
    version: 1,
    sha256: hash(original),
    bytes: changed.length,
    documentSha256: hash(changed),
    intentSha256: intent,
  };
  const begin = await f.tool('app_binary_begin', args);
  await f.tool('app_binary_append', {
    uploadId: begin.uploadId,
    index: 0,
    data: encode64(changed),
  });
  await assert.rejects(f.tool('app_binary_commit', { uploadId: begin.uploadId }));
  assert.deepEqual(await f.tool('app_binary_abort', { uploadId: begin.uploadId }), {
    aborted: false,
    outcome: 'unknown',
    uploadId: begin.uploadId,
    mutationId: 'unknown',
  });
  await assert.rejects(f.tool('app_binary_begin', { ...args, mutationId: 'replacement' }));
  const retried = await f.tool('app_binary_commit', { uploadId: begin.uploadId });
  assert.equal((retried.head as { version: string }).version, '2');
  assert.equal(attempts, 2);
  assert.deepEqual(f.saves[0], f.saves[1]);
  assert.equal(
    (await f.tool('app_binary_abort', { uploadId: begin.uploadId })).outcome,
    'committed',
  );
  const fresh = await fixture();
  t.after(fresh.dispose);
  const local = await fresh.tool('app_binary_begin', args);
  assert.equal(
    (await fresh.tool('app_binary_abort', { uploadId: local.uploadId })).outcome,
    'locallyCancelled',
  );
  assert.equal(fresh.saves.length, 0);
});

test('a rejected save before any durable receipt is conservatively unknown and retains exact retry input', async (t) => {
  let refuse = true;
  const f = await fixture({
    save: async (input) => {
      if (refuse) throw new Error('synthetic pre-write denial');
      return dto(new Uint8Array(Buffer.from(input.data_base64, 'base64')), 2);
    },
  });
  t.after(f.dispose);
  const begin = await f.tool('app_binary_begin', {
    mutationId: 'denied',
    version: 1,
    sha256: hash(original),
    bytes: changed.length,
    documentSha256: hash(changed),
    intentSha256: intent,
  });
  await f.tool('app_binary_append', {
    uploadId: begin.uploadId,
    index: 0,
    data: encode64(changed),
  });
  await assert.rejects(f.tool('app_binary_commit', { uploadId: begin.uploadId }));
  assert.equal((await f.tool('app_binary_abort', { uploadId: begin.uploadId })).outcome, 'unknown');
  refuse = false;
  await f.tool('app_binary_commit', { uploadId: begin.uploadId });
  assert.deepEqual(f.saves[0], f.saves[1]);
});

test('failed initial open retires its Session and leaves real SDK loading-screen close and tool handlers intact', async (t) => {
  const f = await fixture({
    read: async () => {
      throw new Error('initial canonical read unavailable');
    },
  });
  t.after(f.dispose);
  const engine = enginePort();
  let closes = 0;
  const initialClose = async () => {
    closes++;
    return {};
  };
  const initialTool = async () => ({ content: [] });
  f.app.onteardown = initialClose;
  f.app.oncalltool = initialTool;
  await assert.rejects(
    attachDeckcraftDocument({ app: f.app, engine, itemId: 'it_deck', id: () => 'seed' }),
    /initial canonical read unavailable/,
  );
  assert.equal(f.app.onteardown, initialClose);
  assert.equal(f.app.oncalltool, initialTool);
  await assert.rejects(
    engine.zoenHostRequest('{"op":"status"}', new Uint8Array()),
    /engine retired/,
  );
  await f.session.flushResource();
  assert.equal(closes, 1);
});
