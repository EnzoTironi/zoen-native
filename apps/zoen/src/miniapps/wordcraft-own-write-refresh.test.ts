import test from 'node:test';
import assert from 'node:assert/strict';
import { ownWriteFixture, firstWord, newerWord } from './wordcraft-own-write-refresh.fixtures.ts';
import { hash } from './canonical-scene.ts';
import { tick } from './canonical-binary.fixtures.ts';

test('Core publication before own ACK never transiently blocks the live frame or pauses newer input', async (t) => {
  const f = await ownWriteFixture();
  t.after(f.close);
  f.port.type(firstWord);
  const saving = f.owner.document.save();
  void saving.catch(() => {});
  await Promise.all([f.published, f.notification]);
  await tick();
  assert.equal(f.canonical().version, 2);
  assert.equal(
    (f.port.status().head as { version: string }).version,
    '1',
    'Core has published v2 before engine ACK',
  );
  assert.equal(
    f.states.some((state) => state.blocked),
    false,
    'own publication must not hide/inert the focused frame while its ACK is in flight',
  );
  assert.equal(f.events.includes('freeze:true'), false);
  f.port.type(newerWord);
  f.releaseAck();
  await saving;
  await f.refreshes();
  assert.equal(f.writes(), 2);
  assert.deepEqual(f.port.status().head, { version: '3', sha256: hash(newerWord) });
  assert.equal(f.port.status().dirty, false);
  assert.equal(f.port.accepts(), 2);
  assert.equal(f.events.includes('open'), false, 'own saves preserve the same Session/Undo');
  assert.equal(f.events.includes('freeze:true'), false);
  assert.equal(
    f.states.some((state) => state.blocked),
    false,
  );
  await f.session.refresh(2);
  await f.session.refresh(3);
  assert.equal(
    f.states.some((state) => state.blocked),
    false,
    'stale and acknowledged own heads remain observational',
  );
});

test('a peer head published during the own ACK still preserves the local pending draft and fences recovery', async (t) => {
  const f = await ownWriteFixture();
  t.after(f.close);
  f.port.type(firstWord);
  const saving = f.owner.document.save();
  void saving.catch(() => {});
  await Promise.all([f.published, f.notification]);
  f.port.type(newerWord);
  await f.peer();
  assert.equal(f.canonical().version, 3);
  f.releaseAck();
  await assert.rejects(saving);
  await assert.rejects(f.refreshes());
  assert.deepEqual(f.port.status().head, { version: '2', sha256: hash(firstWord) });
  assert.equal(f.port.status().dirty, true);
  assert.ok(f.port.status().pendingMutationId);
  assert.equal(f.port.status().frozen, false);
  assert.equal(
    f.events.includes('open'),
    false,
    'a peer notification cannot replace a local draft',
  );
  assert.equal(f.states.at(-1)?.blocked, true, 'a real unresolved conflict still covers recovery');
});
