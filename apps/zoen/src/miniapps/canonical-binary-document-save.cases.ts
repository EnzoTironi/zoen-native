import assert from 'node:assert/strict';
import test from 'node:test';
import type { AppBinaryDocumentDto } from '../core/generated/AppBinaryDocumentDto';
import {
  changed,
  dto,
  enginePort,
  fixture,
  intent,
  original,
  scope,
  tick,
} from './canonical-binary.fixtures.ts';
import { encode64, hash } from './canonical-scene.ts';
import { attachDeckcraftDocument, DeckcraftDocument } from './deckcraft-document.ts';

test('real official SDK reaches the production scoped binary host with native-shape bytes and exact receipt', async (t) => {
  const f = await fixture();
  t.after(f.dispose);
  await assert.rejects(f.tool('app_binary_commit'), /Invalid App proposal identifier/);
  const read = await f.tool('app_binary_read');
  assert.equal(read.sha256, hash(original));
  assert.deepEqual(read.scope, scope);
  const chunk = await f.tool('app_binary_read_chunk', { transferId: read.transferId, index: 0 });
  assert.deepEqual(Buffer.from(chunk.data as string, 'base64'), Buffer.from(original));
  const begin = await f.tool('app_binary_begin', {
    mutationId: 'save_once',
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
  const result = await f.tool('app_binary_commit', { uploadId: begin.uploadId });
  assert.deepEqual(result.head, { version: '2', sha256: hash(changed) });
  assert.equal(f.saves[0].intent_sha256, intent);
  await f.tool('app_binary_commit', { uploadId: begin.uploadId });
  assert.equal(f.saves.length, 1);
});
test('same production document client freezes before final snapshot and requires explicit resume', async (t) => {
  const f = await fixture();
  t.after(f.dispose);
  const engine = enginePort();
  let ids = 0;
  const document = new DeckcraftDocument({
    app: f.app,
    engine,
    itemId: 'it_deck',
    id: () => `mutation_${++ids}`,
  });
  t.after(() => document.retireAuthority());
  await document.open();
  engine.type(changed);
  await document.flush();
  assert.equal(f.current().version, 2);
  assert.equal(engine.status().dirty, false);
  assert.equal(engine.status().frozen, true);
  assert.throws(() => engine.type(original), /frozen/);
  await document.resume();
  engine.type(original);
  await document.flush();
  assert.equal(f.current().version, 3);
});

test('guest autosave marks only exact interaction refusal as busy, keeps bytes editable, and persists the original document on acknowledged retry', async (t) => {
  const f = await fixture();
  t.after(f.dispose);
  const engine = enginePort();
  const request = engine.zoenHostRequest;
  let refusal: unknown = '{"code":"human_interaction"}';
  engine.zoenHostRequest = async (envelope, bytes) => {
    if (JSON.parse(envelope).op === 'snapshot' && refusal !== undefined) throw refusal;
    return request(envelope, bytes);
  };
  const states: { phase: string; reason?: string }[] = [];
  let ids = 0;
  const document = new DeckcraftDocument({
    app: f.app,
    engine,
    itemId: 'it_deck',
    id: () => `feedback_${++ids}`,
    onState: (phase, _error, reason) => states.push({ phase, reason }),
  });
  t.after(() => document.retireAuthority());
  await document.open();
  t.mock.timers.enable({ apis: ['setTimeout'] });
  engine.type(changed);
  t.mock.timers.tick(220);
  await tick();
  assert.deepEqual(states.at(-1), { phase: 'error', reason: 'autosave_busy' });
  assert.equal(f.saves.length, 0);
  assert.equal(engine.status().frozen, false);
  assert.equal(engine.status().dirty, true);
  for (const failure of [
    new Error('{"code":"human_interaction"}'),
    '{"code":"human_interaction","detail":"private text"}',
    '{"code":"permission_denied"}',
    new Error('private storage path and document text'),
  ]) {
    refusal = failure;
    engine.type(changed);
    t.mock.timers.tick(220);
    await tick();
    assert.deepEqual(states.at(-1), { phase: 'error', reason: 'autosave_failed' });
    assert.equal(f.saves.length, 0);
    assert.equal(engine.status().dirty, true);
  }
  assert.equal(JSON.stringify(states).includes('private'), false);
  refusal = undefined;
  await document.flush();
  assert.equal(f.current().sha256, hash(changed));
  assert.equal(f.current().version, 2);
  assert.equal(engine.status().frozen, true);
  assert.equal(engine.status().dirty, false);
  assert.equal(engine.accepts(), 1);
});

test('guest exact active-interaction Freeze refusal is diagnosed, never acknowledged as clean, and keeps the same document available for retry', async (t) => {
  const f = await fixture();
  t.after(f.dispose);
  const engine = enginePort();
  const request = engine.zoenHostRequest;
  let busy = true;
  engine.zoenHostRequest = async (envelope, bytes) => {
    const input = JSON.parse(envelope);
    if (input.op === 'freeze' && input.input.frozen && busy)
      return Promise.reject('{"code":"human_interaction"}');
    return request(envelope, bytes);
  };
  const states: { phase: string; reason?: string }[] = [];
  const owner = await attachDeckcraftDocument({
    app: f.app,
    engine,
    itemId: 'it_deck',
    id: () => 'freeze_retry',
    onState: (phase, _error, reason) => states.push({ phase, reason }),
  });
  t.after(owner.retireAuthority);
  engine.type(changed);
  await assert.rejects(f.session.flushResource());
  assert.deepEqual(states.at(-1), { phase: 'error', reason: 'flush_busy' });
  assert.equal(f.current().version, 1);
  assert.equal(engine.status().frozen, false);
  assert.equal(engine.status().dirty, true);
  busy = false;
  await f.session.flushResource();
  assert.equal(f.current().sha256, hash(changed));
  assert.equal(f.current().version, 2);
  assert.equal(engine.status().frozen, true);
  assert.equal(engine.accepts(), 1);
});
test('lost durable response preserves exact pending proposal and newer manual typing until retry is accepted', async (t) => {
  const stored = new Map<string, AppBinaryDocumentDto>();
  let calls = 0;
  let head = 1;
  const f = await fixture({
    save: async (input) => {
      calls++;
      const prior = stored.get(input.mutation_id);
      if (prior) return { ...prior, current_version: head };
      const saved = dto(new Uint8Array(Buffer.from(input.data_base64, 'base64')), ++head);
      stored.set(input.mutation_id, saved);
      if (calls === 1) throw new Error('synthetic response lost after durable commit');
      return saved;
    },
  });
  t.after(f.dispose);
  const engine = enginePort();
  let ids = 0;
  const document = new DeckcraftDocument({
    app: f.app,
    engine,
    itemId: 'it_deck',
    id: () => `mutation_${++ids}`,
  });
  t.after(() => document.retireAuthority());
  await document.open();
  engine.type(changed);
  await assert.rejects(document.save());
  engine.type(original);
  await document.flush();
  assert.equal(head, 3);
  assert.equal(calls, 3);
  assert.equal(engine.accepts(), 2);
  assert.deepEqual(f.saves[0], f.saves[1]);
  assert.equal(f.saves[2].expected_version, 2);
  assert.equal(engine.status().dirty, false);
  assert.equal(engine.status().frozen, true);
});
