import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { ForwardBatchReview } from './generated/ForwardBatchReview';
import { owner, deferred, fixture, ops } from './forward-selection.fixtures.ts';
test('one Send issues one exact canonical batch command; Done waits for durable completion', async () => {
  const f = fixture(),
    review = await f.ready();
  assert.equal(review.ready, true);
  assert.ok(Object.isFrozen(review));
  assert.ok(Object.isFrozen(review.messages));
  assert.ok(Object.isFrozen(review.messages[0]!.message.kind));
  assert.deepEqual(ops(f, 'confirm'), []);
  await f.session.confirm(review);
  assert.equal(ops(f, 'confirm').length, 1);
  assert.deepEqual(ops(f, 'confirm')[0], {
    op: 'confirm',
    source: {
      expected_identity_id: 'alice',
      expected_device_id: 'phone',
      source_space: 'source',
      message_ids: ['a', 'b', 'c'],
    },
    batch: review.batchId,
    revision: review.revision,
  });
  assert.deepEqual(f.sent, []);
  await f.session.finish(f.selected()!);
  assert.equal(f.selected()!.completed, true);
  assert.deepEqual(f.sent, ['target']);
  await f.session.finish(f.selected()!);
  assert.equal(ops(f, 'finish').length, 1);
});

test('partial canonical outcomes require explicit fresh review; accepted destinations stay per-item', async () => {
  const f = fixture(),
    original = await f.ready();
  f.confirm(async () => {
    const partial = { ...f.accept(['a']), ready: false, error: 'Source changed' };
    f.setRecord(partial);
    return partial;
  });
  await f.session.confirm(original);
  assert.equal(f.selected()!.ready, false);
  assert.equal(f.selected()!.error, 'Source changed');
  await f.session.confirm(original);
  await f.session.confirm(f.selected()!);
  assert.equal(ops(f, 'confirm').length, 1);
  await f.session.prepare('other');
  assert.equal(f.selected()!.batchId, original.batchId);
  assert.notEqual(f.selected()!.revision, original.revision);
  assert.equal(f.selected()!.targetId, 'other');
  assert.deepEqual(
    f.selected()!.messages.map((item) => item.targetId),
    ['target', 'other', 'other'],
  );
  assert.equal(f.selected()!.messages[0]!.acceptedMessageId, 'accepted-a');
  assert.equal(ops(f, 'review').length, 2);
});

test('unknown acceptance reads durable batch status without resend or auto-finish', async () => {
  const f = fixture(),
    review = await f.ready();
  f.confirm(async () => {
    f.accept();
    throw Error('Lost acknowledgment');
  });
  await f.session.confirm(review);
  assert.equal(ops(f, 'confirm').length, 1);
  assert.equal(ops(f, 'status').length, 2);
  assert.equal(
    f.selected()!.messages.every((item) => item.acceptedMessageId !== null),
    true,
  );
  assert.equal(f.selected()!.completed, false);
  assert.deepEqual(f.sent, []);
  f.session.blur();
  await f.session.focus();
  assert.equal(ops(f, 'confirm').length, 1);
  assert.equal(ops(f, 'review').length, 1);
  assert.equal(f.selected()!.completed, false);
});

test('completed recovery creates a new intent only after an explicit recipient review', async () => {
  const f = fixture(),
    original = await f.ready();
  await f.session.confirm(original);
  await f.session.finish(f.selected()!);
  f.session.blur();
  await f.session.focus();
  assert.equal(f.selected()!.completed, true);
  assert.equal(f.selected()!.ready, false);
  assert.equal(ops(f, 'review').length, 1);
  await f.session.confirm(f.selected()!);
  assert.equal(ops(f, 'confirm').length, 1);
  await f.session.prepare('other');
  assert.notEqual(f.selected()!.batchId, original.batchId);
  assert.equal(f.selected()!.ready, true);
  assert.equal(
    f.selected()!.messages.every((item) => item.acceptedMessageId === null),
    true,
  );
});

test('finish and abandon require source authority but never old destination write/access', async () => {
  const f = fixture(),
    review = await f.ready();
  await f.session.confirm(review);
  f.spaces(new Set(['source']));
  f.writable(false);
  f.session.refresh();
  assert.ok(f.selected());
  await f.session.finish(f.selected()!);
  assert.deepEqual(f.sent, ['target']);
  const partial = fixture(),
    pending = await partial.ready();
  partial.confirm(async () => partial.accept(['a']));
  await partial.session.confirm(pending);
  partial.spaces(new Set(['source']));
  partial.writable(false);
  await partial.session.abandon(partial.selected()!);
  assert.deepEqual(partial.selected()!.skippedIds, ['b', 'c']);
  assert.equal(partial.selected()!.completed, true);
  assert.equal(partial.selected()!.messages[0]!.acceptedMessageId, 'accepted-a');
  assert.deepEqual(partial.sent, []);
});

test('abandon uses exact batch token, publishes distinct skipped outcomes and never sends', async () => {
  const f = fixture(),
    stale = await f.ready();
  await f.session.prepare('other');
  await f.session.abandon(stale);
  assert.equal(ops(f, 'abandon').length, 0);
  const review = f.selected()!;
  await f.session.abandon(review);
  assert.equal(ops(f, 'abandon')[0]!.batch, review.batchId);
  assert.equal(ops(f, 'abandon')[0]!.revision, review.revision);
  assert.deepEqual(f.selected()!.skippedIds, ['a', 'b', 'c']);
  assert.equal(
    f.selected()!.messages.every((item) => item.acceptedMessageId === null),
    true,
  );
  assert.deepEqual(f.sent, []);
  await f.session.abandon(f.selected()!);
  await f.session.abandon(review);
  assert.equal(ops(f, 'abandon').length, 1);
});

test('opening the recipient picker retires stale Send and Done references', async () => {
  const f = fixture(),
    review = await f.ready();
  f.session.clearReview();
  await f.session.confirm(review);
  assert.equal(ops(f, 'confirm').length, 0);
  await f.session.finish(review);
  assert.equal(ops(f, 'finish').length, 0);
});

test('scope, retirement, access and focus loss during awaits suppress callbacks and follow-up calls', async () => {
  for (const change of ['owner', 'device', 'epoch', 'source', 'retiring', 'blur'] as const) {
    const f = fixture(),
      review = await f.ready(),
      waiting = deferred<ForwardBatchReview>();
    f.confirm(() => waiting.promise);
    const sending = f.session.confirm(review);
    if (change === 'owner') f.actor({ ...owner, identityId: 'bob' });
    if (change === 'device') f.actor({ ...owner, deviceId: 'tablet' });
    if (change === 'epoch') f.epoch(2);
    if (change === 'source') f.spaces(new Set(['target']));
    if (change === 'retiring') f.actor(null);
    if (change === 'blur') f.session.blur();
    f.session.refresh();
    waiting.resolve(f.accept());
    await sending;
    assert.equal(f.selected(), null, change);
    assert.equal(ops(f, 'status').length, 1, change);
    assert.deepEqual(f.sent, [], change);
  }
});

test('recipient write loss during confirmation invalidates in-flight publication and clears busy', async () => {
  const f = fixture(),
    review = await f.ready(),
    waiting = deferred<ForwardBatchReview>();
  f.confirm(() => waiting.promise);
  const sending = f.session.confirm(review);
  f.writable(false);
  f.session.refresh();
  waiting.resolve(f.accept());
  await sending;
  assert.equal(f.selected()!.ready, false);
  assert.equal(f.state().busy, false);
  assert.deepEqual(f.sent, []);
});

test('blurred recovery and a rejected reconciliation never expose a sendable stale review', async () => {
  const f = fixture(),
    waiting = deferred<ForwardBatchReview | null>();
  f.status(() => waiting.promise);
  const focus = f.session.focus();
  f.session.blur();
  waiting.resolve(null);
  await focus;
  assert.equal(f.selected(), null);
  assert.equal(ops(f, 'review').length, 0);
  const failed = fixture(),
    review = await failed.ready();
  failed.confirm(async () => {
    throw Error('Unknown acceptance');
  });
  failed.status(async () => {
    throw Error('Status unavailable');
  });
  await failed.session.confirm(review);
  assert.equal(failed.selected()!.ready, false);
  assert.equal(ops(failed, 'confirm').length, 1);
});

test('untrusted result set cannot change the reviewed source identity or retain mutable message aliases', async () => {
  const f = fixture();
  await f.ready();
  const original = f.selected()!.messages[0]!.message;
  f.record().messages[0]!.message.deleted = true;
  assert.equal(original.deleted, false);
  f.setRecord({ ...f.record(), messages: [f.record().messages[0]!] });
  await f.session.recover();
  assert.ok(f.state().error instanceof Error);
  assert.equal(f.selected()!.ready, false);
});
