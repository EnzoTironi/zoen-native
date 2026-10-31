import type { ForwardReview as Canonical } from './generated/ForwardReview';
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { deferred, message, fixture } from './forward-message.fixtures.ts';
test('destination choice only reviews; explicit Send confirms once then ACKs exact live acceptance', async () => {
  const f = fixture();
  await f.ready();
  await f.session.prepare('target');
  const review = f.selected()!;
  assert.deepEqual(review.message, message);
  assert.equal(f.confirms.length, 0);
  await f.session.confirm(review);
  assert.deepEqual(f.confirms, [review.operationId]);
  assert.deepEqual(f.dismisses, ['accepted']);
  assert.deepEqual(f.sends, ['target']);
  await f.session.confirm(review);
  assert.equal(f.confirms.length, 1);
});
test('Cancel fences stale Send and the next destination gets a distinct canonical operation', async () => {
  const f = fixture();
  await f.ready();
  await f.session.prepare('target');
  const old = f.selected()!;
  await f.session.cancelReview();
  assert.deepEqual(f.dismisses, [null]);
  await f.session.prepare('other');
  await f.session.confirm(old);
  assert.equal(f.confirms.length, 0);
  assert.notEqual(f.selected()?.operationId, old.operationId);
});
test('unknown primary ACK reconciles acceptance without cleanup ACK or another send', async () => {
  const f = fixture();
  await f.ready();
  await f.session.prepare('target');
  const review = f.selected()!;
  f.confirm(async (value) => {
    f.accept(value);
    throw Error('Response lost after commit');
  });
  await f.session.confirm(review);
  assert.deepEqual(f.sends, ['target']);
  assert.deepEqual(f.dismisses, []);
  assert.equal(f.selected()?.acceptedMessageId, 'accepted');
  await f.session.confirm(review);
  await f.session.confirm(f.selected()!);
  assert.equal(f.confirms.length, 1);
});
test('failed live cleanup remains Sent and retains canonical recovery', async () => {
  const f = fixture();
  await f.ready();
  await f.session.prepare('target');
  f.dismiss(async () => {
    throw Error('Cleanup lost');
  });
  await f.session.confirm(f.selected()!);
  assert.deepEqual(f.sends, ['target']);
  assert.equal(f.state().error, null);
  assert.equal(f.record()?.accepted_message_id, 'accepted');
});
test('cold recovery exposes exact destination/accepted marker and Forward again is explicit', async () => {
  const f = fixture();
  await f.ready();
  await f.session.prepare('target');
  const original = f.record()!;
  f.accept(original);
  f.session.blur();
  await f.ready();
  assert.equal(f.selected()?.targetId, 'target');
  assert.equal(f.selected()?.acceptedMessageId, 'accepted');
  assert.deepEqual(f.sends, []);
  assert.deepEqual(f.dismisses, []);
  await f.session.cancelReview();
  assert.deepEqual(f.dismisses, ['accepted']);
  await f.session.prepare('other');
  assert.notEqual(f.selected()?.operationId, original.operation_id);
});
test('a retained operation wins over a new destination selection', async () => {
  const f = fixture();
  await f.ready();
  await f.session.prepare('target');
  const op = f.selected()?.operationId;
  await f.session.prepare('other');
  assert.equal(f.selected()?.targetId, 'target');
  assert.equal(f.selected()?.operationId, op);
  assert.deepEqual(f.reviews, ['target']);
});
for (const stage of ['lookup', 'confirm'] as const)
  test(`blur during ${stage} suppresses all late UI and receipt cleanup`, async () => {
    const f = fixture();
    await f.ready();
    if (stage === 'lookup') {
      const p = deferred<Canonical | null>();
      f.status(() => p.promise);
      const task = f.session.prepare('target');
      f.session.blur();
      p.resolve(null);
      await task;
      assert.equal(f.reviews.length, 0);
    } else {
      await f.session.prepare('target');
      const p = deferred<Canonical>();
      f.confirm(async (value) => {
        f.accept(value);
        return p.promise;
      });
      const task = f.session.confirm(f.selected()!);
      f.session.blur();
      p.resolve(f.record()!);
      await task;
    }
    assert.deepEqual(f.sends, []);
    assert.deepEqual(f.dismisses, []);
    assert.equal(f.selected(), null);
  });
test('unreadable status after unknown ACK never mints a new intent or auto-resends', async () => {
  const f = fixture();
  await f.ready();
  await f.session.prepare('target');
  const review = f.selected()!;
  f.confirm(async () => {
    throw Error('Unknown');
  });
  f.status(async () => {
    throw Error('Unreadable');
  });
  await f.session.confirm(review);
  assert.ok(f.state().error instanceof Error);
  assert.equal(f.selected(), review);
  await f.session.prepare('other');
  assert.equal(f.reviews.length, 1);
  assert.equal(f.confirms.length, 1);
});
test('changed source requires explicit new canonical review, never a frontend fingerprint', async () => {
  const f = fixture();
  await f.ready();
  await f.session.prepare('target');
  const review = f.selected()!;
  f.revision('r2');
  await f.session.confirm(review);
  assert.ok(f.state().error instanceof Error);
  assert.equal(f.selected()?.operationId, review.operationId);
  await f.session.cancelReview();
  await f.session.prepare('target');
  assert.equal(f.selected()?.sourceRevision, 'r2');
});
for (const revoked of ['owner', 'epoch', 'membership', 'writer'])
  test(`revoked ${revoked} blocks retained Send without discarding Core state`, async () => {
    const f = fixture();
    await f.ready();
    await f.session.prepare('target');
    const review = f.selected()!;
    f.revoke(revoked);
    f.session.refresh();
    await f.session.confirm(review);
    assert.equal(f.confirms.length, 0);
    assert.deepEqual(f.dismisses, []);
  });
test('Cancel losing the acceptance race preserves receipt and projects accepted instead of another intent', async () => {
  const f = fixture();
  await f.ready();
  await f.session.prepare('target');
  f.accept(f.record()!);
  await f.session.cancelReview();
  assert.equal(f.selected()?.acceptedMessageId, 'accepted');
  assert.equal(f.state().error, null);
  assert.equal(f.record()?.accepted_message_id, 'accepted');
  assert.equal(f.confirms.length, 0);
});
test('parallel explicit Send handlers share one in-flight canonical confirmation', async () => {
  const f = fixture();
  await f.ready();
  await f.session.prepare('target');
  const review = f.selected()!;
  const p = deferred<Canonical>();
  f.confirm(async (value) => {
    f.accept(value);
    return p.promise;
  });
  const first = f.session.confirm(review);
  await f.session.confirm(review);
  assert.equal(f.confirms.length, 1);
  p.resolve(f.record()!);
  await first;
  assert.deepEqual(f.sends, ['target']);
});

test('a lost explicit Cancel ACK reconciles absence without an error or replacement intent', async () => {
  const f = fixture();
  await f.ready();
  await f.session.prepare('target');
  // Model a private metadata delete that committed before its bridge response was lost.
  f.dismiss(async () => {
    f.status(async () => null);
    throw Error('Cancel ACK lost');
  });
  await f.session.cancelReview();
  assert.equal(f.selected(), null);
  assert.equal(f.state().error, null);
  assert.equal(f.confirms.length, 0);
  assert.equal(f.reviews.length, 1);
});
