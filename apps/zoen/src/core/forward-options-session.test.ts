import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fixture as single, deferred } from './forward-message.fixtures.ts';
import { fixture as batch, owner, ops } from './forward-selection.fixtures.ts';
import type { ForwardReview } from './generated/ForwardReview';
import type { ForwardBatchReview } from './generated/ForwardBatchReview';

const hidden = { hide_author: true, hide_caption: true };

test('single options rotate the reviewed intent; only explicit Send with the new review can publish', async () => {
  const f = single();
  await f.ready();
  await f.session.prepare('target');
  const previous = f.selected()!;
  const input = { ...hidden };
  await f.session.configure(previous, input);
  input.hide_author = false;
  const current = f.selected()!;
  assert.notEqual(current.operationId, previous.operationId);
  assert.equal(current.sourceRevision, previous.sourceRevision);
  assert.deepEqual(current.options, hidden);
  assert.ok(Object.isFrozen(current.options));
  assert.deepEqual(f.configurations, [
    {
      id: previous.operationId,
      revision: previous.sourceRevision,
      options: hidden,
    },
  ]);
  assert.deepEqual(f.confirms, []);
  assert.deepEqual(f.sends, []);
  await f.session.confirm(previous);
  await f.session.configure(previous, hidden);
  assert.equal(f.configurations.length, 1);
  assert.deepEqual(f.confirms, []);
  await f.session.confirm(current);
  assert.deepEqual(f.confirms, [current.operationId]);
  assert.deepEqual(f.sends, ['target']);
  await f.session.configure(f.selected()!, hidden);
  assert.equal(f.configurations.length, 1, 'accepted records are immutable');
});

test('a lost single configuration ACK recovers canonical flags and never sends automatically', async () => {
  const f = single();
  await f.ready();
  await f.session.prepare('target');
  const previous = f.selected()!;
  f.configure(async () => {
    throw Error('Lost configuration ACK');
  });
  await f.session.configure(previous, hidden);
  assert.deepEqual(f.selected()!.options, hidden);
  assert.notEqual(f.selected()!.operationId, previous.operationId);
  assert.match(String(f.state().error), /Lost configuration ACK/);
  assert.deepEqual(f.sends, []);
  assert.deepEqual(f.confirms, []);
  f.session.blur();
  await f.ready();
  assert.deepEqual(f.selected()!.options, hidden);
  await f.session.confirm(previous);
  assert.deepEqual(f.confirms, []);
});

for (const reason of ['blur', 'owner', 'epoch', 'membership', 'writer'])
  test(`single configuration cannot publish late UI or send after ${reason}`, async () => {
    const f = single();
    await f.ready();
    await f.session.prepare('target');
    const previous = f.selected()!;
    const response = deferred<ForwardReview>();
    f.configure(() => response.promise);
    const running = f.session.configure(previous, hidden);
    await f.session.confirm(previous);
    assert.deepEqual(f.confirms, [], 'in-flight options own the operation');
    if (reason === 'blur') f.session.blur();
    else {
      f.revoke(reason);
      f.session.refresh();
    }
    response.resolve(f.record()!);
    await running;
    assert.notEqual(f.selected()?.operationId, f.record()!.operation_id);
    assert.deepEqual(f.sends, []);
    assert.deepEqual(f.confirms, []);
  });

test('single review owns its nested canonical source without freezing or borrowing the caller', async () => {
  const f = single();
  await f.ready();
  await f.session.prepare('target');
  const record = f.record()!;
  record.message = JSON.parse(JSON.stringify(record.message));
  await f.session.recover();
  const viewed = f.selected()!;
  assert.ok('Message' in record.message.kind && 'Message' in viewed.message.kind);
  record.message.kind.Message.text = 'Caller recycled';
  record.options.hide_caption = true;
  assert.equal(viewed.message.kind.Message.text, 'Exact source');
  assert.equal(viewed.options.hide_caption, false);
  assert.ok(!Object.isFrozen(record.message.kind.Message));
  assert.ok(Object.isFrozen(viewed.message.kind.Message));
});

test('partial batch options preserve accepted rows and never re-arm interrupted work', async () => {
  const f = batch();
  await f.ready();
  f.accept(['a']);
  await f.session.recover();
  const previous = f.selected()!;
  assert.equal(previous.ready, false);
  await f.session.configure(previous, hidden);
  const current = f.selected()!;
  assert.notEqual(current.revision, previous.revision);
  assert.equal(current.batchId, previous.batchId);
  assert.equal(current.ready, false);
  assert.deepEqual(current.messages[0], previous.messages[0]);
  assert.deepEqual(current.messages[1]!.options, hidden);
  assert.deepEqual(current.messages[2]!.options, hidden);
  await f.session.confirm(previous);
  await f.session.confirm(current);
  assert.equal(ops(f, 'confirm').length, 0);
  assert.deepEqual(f.sent, []);
  await f.session.configure(previous, hidden);
  assert.equal(ops(f, 'configure').length, 1);
});

test('a lost batch configuration ACK restores canonical options without confirming or finishing', async () => {
  const f = batch();
  const previous = await f.ready();
  f.configure(async () => {
    throw Error('Lost configuration ACK');
  });
  await f.session.configure(previous, hidden);
  assert.notEqual(f.selected()!.revision, previous.revision);
  assert.ok(f.selected()!.messages.every((r) => r.options.hide_author && r.options.hide_caption));
  assert.match(String(f.state().error), /Lost configuration ACK/);
  assert.deepEqual(f.sent, []);
  assert.equal(ops(f, 'confirm').length, 0);
  assert.equal(ops(f, 'finish').length, 0);
  await f.session.confirm(previous);
  assert.equal(ops(f, 'confirm').length, 0);
});

for (const reason of ['blur', 'owner', 'device', 'epoch', 'membership', 'writer', 'retarget'])
  test(`batch configuration cannot reuse late work after ${reason}`, async () => {
    const f = batch();
    const previous = await f.ready();
    const response = deferred<ForwardBatchReview>();
    f.configure(() => response.promise);
    const running = f.session.configure(previous, hidden);
    await f.session.confirm(previous);
    assert.equal(ops(f, 'confirm').length, 0);
    if (reason === 'blur') f.session.blur();
    if (reason === 'retarget') f.session.clearReview();
    if (reason === 'owner') f.actor({ ...owner, identityId: 'foreign' });
    if (reason === 'device') f.actor({ ...owner, deviceId: 'foreign' });
    if (reason === 'epoch') f.epoch(2);
    if (reason === 'membership') f.spaces(new Set(['source']));
    if (reason === 'writer') f.writable(false);
    f.session.refresh();
    response.resolve(f.record());
    await running;
    assert.notEqual(f.selected()?.revision, f.record().review_revision);
    assert.equal(ops(f, 'confirm').length, 0);
    assert.deepEqual(f.sent, []);
  });
