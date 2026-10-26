import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { HomeOwner } from '../home/preferences';
import type { TimelineEntry } from './types';
import type { ForwardReview as Canonical } from './generated/ForwardReview';
import { ForwardMessageSession, type ForwardMessageReview } from './forward-message.ts';
const owner: HomeOwner = { mode: 'account', identityId: 'alice', deviceId: 'phone' };
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
const message = {
  id: 'message',
  kind: { Message: { text: 'Exact source', card: null } },
  deleted: false,
} as TimelineEntry;
function fixture() {
  let actor: HomeOwner | null = owner,
    epoch = 1,
    writable = true,
    spaces = new Set(['source', 'target', 'other']);
  let record: Canonical | null = null,
    next = 0,
    revision = 'r1',
    selected: ForwardMessageReview | null = null,
    error: unknown = null,
    busy = false;
  const sends: string[] = [],
    confirms: string[] = [],
    dismisses: (string | null)[] = [],
    reviews: string[] = [];
  let statusHook: (() => Promise<Canonical | null>) | null = null,
    confirmHook: ((value: Canonical) => Promise<Canonical>) | null = null,
    dismissHook: (() => Promise<void>) | null = null;
  const accept = (value: Canonical) =>
    (record = { ...value, accepted_message_id: 'accepted', accepted: message });
  const session = new ForwardMessageSession(
    { owner, spaceId: 'source', messageId: 'message', sessionEpoch: 1 },
    {
      owner: () => actor,
      epoch: () => epoch,
      writable: () => writable,
      spaces: () => spaces,
      status: async (scope) => {
        assert.equal(scope.source_space, 'source');
        return statusHook ? statusHook() : record;
      },
      review: async (scope) => {
        reviews.push(scope.target_space);
        return (record ??= {
          target_space: scope.target_space,
          operation_id: `op${++next}`,
          source_revision: revision,
          message,
          accepted: null,
          accepted_message_id: null,
        });
      },
      confirm: async (scope, id, rev) => {
        confirms.push(id);
        assert.equal(scope.expected_identity_id, owner.identityId);
        assert.equal(id, record?.operation_id);
        assert.equal(scope.target_space, record?.target_space);
        if (!record) throw Error('Missing');
        if (confirmHook) return confirmHook(record);
        if (record.accepted_message_id) return record;
        if (rev !== revision) throw Error('Changed');
        return accept(record);
      },
      dismiss: async (scope, id, accepted) => {
        dismisses.push(accepted);
        if (dismissHook) return dismissHook();
        if (!record) return;
        if (
          scope.target_space !== record.target_space ||
          id !== record.operation_id ||
          accepted !== record.accepted_message_id
        )
          throw Error('CAS lost');
        record = null;
      },
    },
    {
      busy: (v) => {
        busy = v;
      },
      error: (v) => {
        error = v;
      },
      reviewed: (v) => {
        selected = v;
      },
      sent: (v) => {
        sends.push(v);
      },
    },
  );
  return {
    session,
    ready: () => session.focus(),
    selected: () => selected,
    record: () => record,
    state: () => ({ error, busy }),
    sends,
    confirms,
    dismisses,
    reviews,
    accept,
    status: (hook: typeof statusHook) => {
      statusHook = hook;
    },
    confirm: (hook: typeof confirmHook) => {
      confirmHook = hook;
    },
    dismiss: (hook: typeof dismissHook) => {
      dismissHook = hook;
    },
    revision: (v: string) => {
      revision = v;
    },
    revoke: (kind: string) => {
      if (kind === 'owner') actor = { ...owner, identityId: 'foreign' };
      if (kind === 'epoch') epoch = 2;
      if (kind === 'membership') spaces = new Set(['source']);
      if (kind === 'writer') writable = false;
    },
  };
}
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
