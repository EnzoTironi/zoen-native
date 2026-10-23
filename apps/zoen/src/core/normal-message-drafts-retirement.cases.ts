import assert from 'node:assert/strict';
import { test } from 'node:test';
import { deferred, fixture, key, kind, main } from './normal-message-drafts.fixtures.ts';
import { NormalMessageDrafts } from './normal-message-drafts.ts';

test('failed load cannot autosave over ciphertext, and explicit retry restores the same draft', async () => {
  const f = fixture();
  f.stored.set(key(main), { revision: 'saved-token', text: 'Recover this', reply_to: 'quote' });
  f.hooks.load = async () => {
    throw new Error('Synthetic encrypted read failure');
  };
  await assert.rejects(f.drafts.activate(main), kind('load'));
  assert.equal(f.drafts.setText(main, 'Overwrite'), false);
  assert.equal(f.drafts.getSnapshot(main).loaded, false);
  assert.equal(f.calls.filter((call) => call.op === 'save').length, 0);
  f.hooks.load = undefined;
  await f.drafts.retry(main);
  assert.equal(f.drafts.getSnapshot(main).text, 'Recover this');
  assert.equal(f.drafts.getSnapshot(main).replyTo, 'quote');
});

test('lost save response reconciles its exact accepted payload before saving newer local text', async () => {
  const f = fixture();
  await f.drafts.activate(main);
  f.hooks.saved = async () => {
    f.hooks.saved = undefined;
    throw new Error('Synthetic save response lost');
  };
  const failed = deferred<void>();
  const unsubscribe = f.drafts.subscribe(() => {
    if (f.drafts.getSnapshot(main).error?.kind === 'save') failed.resolve();
  });
  f.drafts.setText(main, 'First payload');
  await failed.promise;
  unsubscribe();
  f.drafts.setText(main, 'Newer payload');
  await f.drafts.retry(main);
  assert.equal(f.current(main).text, 'Newer payload');
  assert.equal(f.drafts.getSnapshot(main).text, 'Newer payload');
  assert.equal(f.drafts.getSnapshot(main).error, null);
});

test('a foreign CAS conflict is surfaced without overwriting either local or remote content', async () => {
  const f = fixture();
  await f.drafts.activate(main);
  const entered = deferred<void>();
  f.hooks.save = async () => {
    entered.resolve();
    throw new Error('Synthetic save unavailable');
  };
  f.drafts.setText(main, 'Retain local');
  await entered.promise;
  await assert.rejects(f.drafts.flush(main), kind('save'));
  f.hooks.save = undefined;
  f.stored.set(key(main), { revision: 'foreign', text: 'Other writer', reply_to: null });
  const saves = f.calls.filter((call) => call.op === 'save').length;
  await assert.rejects(f.drafts.retry(main), kind('conflict'));
  assert.equal(f.current(main).text, 'Other writer');
  assert.equal(f.drafts.getSnapshot(main).text, 'Retain local');
  assert.equal(f.calls.filter((call) => call.op === 'save').length, saves);
});

test('preserving retirement closes admission synchronously, drains latest text, and can cold restore', async () => {
  const f = fixture();
  await f.drafts.activate(main);
  f.drafts.setText(main, 'Keep on close');
  f.drafts.setReply(main, 'quote');
  const closing = f.drafts.retire({ flush: true });
  assert.equal(f.drafts.setText(main, 'Stale callback'), false);
  await assert.rejects(f.drafts.send(main), kind('closed'));
  await closing;
  assert.equal(f.current(main).text, 'Keep on close');
  const next = new NormalMessageDrafts(f.port);
  await next.activate(main);
  assert.equal(next.getSnapshot(main).text, 'Keep on close');
  assert.equal(next.getSnapshot(main).replyTo, 'quote');
});

test('failed preserving close retains text and permits a later retire retry without reopening setters', async () => {
  const f = fixture();
  await f.drafts.activate(main);
  let attempts = 0;
  f.hooks.save = async () => {
    if (++attempts <= 2) throw new Error('Synthetic disk unavailable');
  };
  f.drafts.setText(main, 'Unsaved on close');
  await assert.rejects(f.drafts.retire({ flush: true }), kind('save'));
  assert.equal(f.drafts.getSnapshot(main).text, 'Unsaved on close');
  assert.equal(f.drafts.setText(main, 'Old callback'), false);
  await f.drafts.retire({ flush: true });
  assert.equal(f.current(main).text, 'Unsaved on close');
  assert.equal(f.drafts.getSnapshot(main).error, null);
});

test('discard escalates failed preserving close and cancels all queued work', async () => {
  const f = fixture();
  await f.drafts.activate(main);
  f.hooks.save = async () => {
    throw new Error('Synthetic disk unavailable');
  };
  f.drafts.setText(main, 'Erase this');
  await assert.rejects(f.drafts.retire({ flush: true }), kind('save'));
  const calls = f.calls.length;
  await f.drafts.retire({ flush: false });
  assert.equal(f.drafts.getSnapshot(main).text, '');
  assert.equal(f.drafts.getSnapshot(main).loaded, false);
  await f.drafts.retire({ flush: true });
  assert.equal(f.calls.length, calls);
});

test('discard drains an admitted save, cancels another queued scope, and never retains late plaintext', async () => {
  const f = fixture();
  const thread = { ...main, threadRootId: 'thread' };
  await f.drafts.activate(main);
  await f.drafts.activate(thread);
  const entered = deferred<void>();
  const gate = deferred<void>();
  f.hooks.save = async () => {
    entered.resolve();
    await gate.promise;
  };
  f.drafts.setText(main, 'SYNTHETIC SECRET TO ERASE');
  await entered.promise;
  f.drafts.setText(thread, 'QUEUED SECRET TO ERASE');
  let drained = false;
  const erasing = f.drafts.retire({ flush: false }).then(() => {
    drained = true;
  });
  assert.equal(drained, false);
  assert.equal(f.drafts.getSnapshot(main).text, '');
  assert.equal(f.drafts.setText(thread, 'Stale callback'), false);
  gate.resolve();
  await erasing;
  assert.equal(drained, true);
  assert.equal(f.calls.filter((call) => call.op === 'save').length, 1);
  // Public redaction alone would miss private retry buffers recreated by a late
  // save failure. Erasure must also release those controller-owned buffers.
  const records = Reflect.get(f.drafts, 'records') as Map<string, unknown>;
  assert.equal(JSON.stringify([...records.values()]).includes('SECRET TO ERASE'), false);
});
