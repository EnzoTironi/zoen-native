import test from 'node:test';
import assert from 'node:assert/strict';
import { ownWriteFixture, firstWord, newerWord } from './wordcraft-own-write-refresh.fixtures.ts';
import { hash } from './canonical-scene.ts';
import { tick } from '../core/store.fixtures.ts';

// Actual SDK/native canonical tool host and document controller; engine and Core
// are explicit behavioral ports. This does not qualify WebKit or iPhone timing.
function hiddenInput(f: Awaited<ReturnType<typeof ownWriteFixture>>) {
  let hidden = true;
  const request = f.port.zoenHostRequest;
  f.port.zoenHostRequest = async (envelope, bytes) => {
    const value = JSON.parse(envelope);
    if (hidden && value.op === 'freeze' && value.input.frozen)
      throw JSON.stringify({ code: 'human_interaction' });
    return request(envelope, bytes);
  };
  return () => {
    hidden = false;
  };
}

test('hidden-frame lifecycle checkpoint defers without claiming saved; visible checkpoint saves the same draft', async (t) => {
  const f = await ownWriteFixture();
  t.after(f.close);
  f.stopNotifications();
  f.releaseAck();
  const foreground = hiddenInput(f);
  f.port.type(firstWord);
  await f.session.checkpoint();
  assert.equal(f.writes(), 0);
  assert.equal(f.states.at(-1)?.phase, 'pending');
  assert.equal(f.states.at(-1)?.error, null);
  assert.equal(f.states.at(-1)?.blocked, false);
  assert.equal(f.port.status().dirty, true);
  assert.equal(f.port.status().frozen, false);
  foreground();
  await f.session.checkpoint();
  assert.equal(f.writes(), 1);
  assert.equal(f.canonical().sha256, hash(firstWord));
  assert.equal(f.port.status().dirty, false);
  assert.equal(f.port.status().frozen, false);
  assert.equal(f.states.at(-1)?.phase, 'saved');
  assert.equal(f.states.at(-1)?.error, null);
});

test('a clean status cannot certify raw input retained by a hidden frame', async (t) => {
  const f = await ownWriteFixture();
  t.after(f.close);
  f.stopNotifications();
  f.releaseAck();
  const foreground = hiddenInput(f);
  // The UI has not drained its queued input into the document model yet.
  assert.equal(f.port.status().dirty, false);
  await f.session.checkpoint();
  assert.equal(f.states.at(-1)?.phase, 'pending');
  assert.equal(f.states.at(-1)?.error, null);
  assert.equal(f.writes(), 0);
  foreground();
  f.port.type(firstWord);
  await f.session.checkpoint();
  assert.equal(f.canonical().sha256, hash(firstWord));
  assert.equal(f.writes(), 1);
  assert.equal(f.states.at(-1)?.phase, 'saved');
});

test('the same hidden input still refuses strict Close and cannot acknowledge final input', async (t) => {
  const f = await ownWriteFixture();
  t.after(f.close);
  hiddenInput(f);
  f.port.type(firstWord);
  await assert.rejects(f.session.controller.flush({ retire: true }));
  assert.equal(f.states.at(-1)?.blocked, true);
  assert.ok(f.states.at(-1)?.error);
  assert.equal(f.writes(), 0);
  assert.equal(f.port.status().dirty, true);
});

test('real canonical refusal preserves exact pending mutation and foreground never retries it', async (t) => {
  const f = await ownWriteFixture();
  t.after(f.close);
  f.stopNotifications();
  const operation = f.driver.operation;
  let attempts = 0;
  f.driver.operation = async (command) => {
    if (command.op === 'app_binary_save') {
      attempts++;
      throw Error('Authority refused');
    }
    return operation(command);
  };
  f.port.type(firstWord);
  await assert.rejects(f.session.checkpoint());
  const mutation = f.port.status().pendingMutationId;
  assert.ok(mutation);
  assert.equal(attempts, 1);
  assert.ok(f.states.at(-1)?.error);
  assert.equal(f.states.at(-1)?.blocked, true);
  await f.session.checkpoint();
  assert.equal(attempts, 1);
  assert.equal(f.port.status().pendingMutationId, mutation);
  assert.equal(f.writes(), 0);
});

test('unknown engine errors are never classified as a deferred input frame', async (t) => {
  for (const failure of [
    Error('{"code":"human_interaction"}'),
    '{"code":"human_interaction","extra":true}',
  ]) {
    const f = await ownWriteFixture();
    t.after(f.close);
    f.port.zoenHostRequest = async () => {
      throw failure;
    };
    await assert.rejects(f.session.checkpoint());
    assert.equal(f.states.at(-1)?.blocked, true);
    assert.ok(f.states.at(-1)?.error);
    assert.equal(f.writes(), 0);
  }
});

test('a checkpoint ACK cannot label newer admitted input as saved', async (t) => {
  const f = await ownWriteFixture();
  t.after(f.close);
  f.stopNotifications();
  f.releaseAck();
  const call = f.app.oncalltool;
  assert.ok(call);
  f.app.oncalltool = async (...args) => {
    const result = await call(...args);
    f.port.type(newerWord);
    await tick();
    return result;
  };
  f.port.type(firstWord);
  await f.session.checkpoint();
  assert.equal(f.canonical().sha256, hash(firstWord));
  assert.equal(f.port.status().dirty, true);
  assert.equal(f.states.at(-1)?.phase, 'pending');
  assert.equal(f.states.at(-1)?.error, null);
});

test('retired owner cannot checkpoint or publish a late lifecycle result', async (t) => {
  const f = await ownWriteFixture();
  t.after(f.close);
  f.session.retire();
  const count = f.states.length;
  await assert.rejects(f.session.checkpoint());
  assert.equal(f.states.length, count);
  assert.equal(f.writes(), 0);
});
