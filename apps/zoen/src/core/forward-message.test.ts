import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { HomeOwner } from '../home/preferences';
import {
  ForwardMessageSession,
  ForwardMessageUnavailable,
  type ForwardMessageReview,
} from './forward-message.ts';
import type { TimelineEntry } from './types';

const ownerA: HomeOwner = { mode: 'account', identityId: 'alice', deviceId: 'phone_a' };
const sourceId = 'source_chat';
const messageId = 'synthetic_message';
const targetId = 'target_chat';
const otherTargetId = 'other_target_chat';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

function message(overrides: Partial<TimelineEntry> = {}): TimelineEntry {
  return {
    id: messageId,
    deleted: false,
    kind: { Message: { text: 'Synthetic forwarding fixture', card: null } },
    ...overrides,
  } as TimelineEntry;
}

type Event =
  | { kind: 'busy'; value: boolean }
  | { kind: 'error'; reason: unknown | null }
  | { kind: 'sent'; target: string };

function fixture(scope: 'current' | 'none' = 'current') {
  let owner: HomeOwner | null = { ...ownerA };
  let spaces = new Set([sourceId, targetId, otherTargetId]);
  let read: () => Promise<TimelineEntry> = async () => message();
  let forward: () => Promise<unknown> = async () => undefined;
  let epoch = 1;
  let writable = true;
  let review: ForwardMessageReview | null = null;
  let busy = false;
  let error: unknown | null = null;
  const events: Event[] = [];
  const reads: { space: string; message: string }[] = [];
  const forwards: { space: string; message: string; target: string }[] = [];
  const entered = deferred<void>();
  const session = new ForwardMessageSession(
    scope === 'current' ? { owner: ownerA, spaceId: sourceId, messageId, sessionEpoch: 1 } : null,
    {
      owner: () => owner,
      spaces: () => spaces,
      epoch: () => epoch,
      writable: () => writable,
      message: async (space, id) => {
        reads.push({ space, message: id });
        return read();
      },
      forward: async (space, id, target) => {
        forwards.push({ space, message: id, target });
        entered.resolve();
        return forward();
      },
    },
    {
      reviewed: (value) => {
        review = value;
      },
      busy: (value) => {
        busy = value;
        events.push({ kind: 'busy', value });
      },
      error: (reason) => {
        error = reason;
        events.push({ kind: 'error', reason });
      },
      sent: (target) => events.push({ kind: 'sent', target }),
    },
  );
  session.focus();
  return {
    session,
    events,
    reads,
    forwards,
    entered,
    state: () => ({ busy, error }),
    review: () => review,
    epoch: (value: number) => {
      epoch = value;
    },
    writable: (value: boolean) => {
      writable = value;
    },
    owner: (value: HomeOwner | null) => {
      owner = value;
    },
    spaces: (value: readonly string[]) => {
      spaces = new Set(value);
    },
    read: (value: () => Promise<TimelineEntry>) => {
      read = value;
    },
    forward: (value: () => Promise<unknown>) => {
      forward = value;
    },
  };
}

test('forwarding rechecks the canonical source and synchronously admits only one destination', async () => {
  const f = fixture();
  const canonical = deferred<TimelineEntry>();
  f.read(() => canonical.promise);
  const first = f.session.send(targetId);
  const duplicate = f.session.send(otherTargetId);
  assert.equal(f.state().busy, true);
  assert.deepEqual(f.reads, [{ space: sourceId, message: messageId }]);
  assert.deepEqual(f.forwards, []);
  await duplicate;
  canonical.resolve(message());
  await first;
  assert.deepEqual(f.forwards, [{ space: sourceId, message: messageId, target: targetId }]);
  assert.deepEqual(f.state(), { busy: false, error: null });
  assert.deepEqual(f.events.slice(-2), [
    { kind: 'sent', target: targetId },
    { kind: 'busy', value: false },
  ]);
});

for (const leave of ['blur', 'dispose'] as const) {
  test(`${leave} during canonical lookup prevents forwarding and all late UI callbacks`, async () => {
    const f = fixture();
    const canonical = deferred<TimelineEntry>();
    f.read(() => canonical.promise);
    const sending = f.session.send(targetId);
    f.session[leave]();
    const afterLeaving = [...f.events];
    canonical.resolve(message());
    await sending;
    assert.deepEqual(f.forwards, []);
    assert.deepEqual(f.events, afterLeaving);
  });

  test(`${leave} after command acceptance cannot navigate or update UI on completion`, async () => {
    const f = fixture();
    const accepted = deferred<void>();
    f.forward(() => accepted.promise);
    const sending = f.session.send(targetId);
    await f.entered.promise;
    f.session[leave]();
    const afterLeaving = [...f.events];
    accepted.resolve();
    await sending;
    assert.equal(f.forwards.length, 1);
    assert.deepEqual(f.events, afterLeaving);
  });
}

test('returning to the same focused route does not revive an old completion or release a newer latch', async () => {
  const f = fixture();
  const firstAccepted = deferred<void>();
  f.forward(() => firstAccepted.promise);
  const old = f.session.send(targetId);
  await f.entered.promise;
  f.session.blur();
  f.session.focus();

  const nextEntered = deferred<void>();
  const nextAccepted = deferred<void>();
  f.forward(() => {
    nextEntered.resolve();
    return nextAccepted.promise;
  });
  const next = f.session.send(otherTargetId);
  await nextEntered.promise;
  assert.equal(f.state().busy, true);
  const whileNewSendIsPending = [...f.events];
  firstAccepted.resolve();
  await old;
  assert.deepEqual(f.events, whileNewSendIsPending);
  assert.equal(f.state().busy, true);
  nextAccepted.resolve();
  await next;
  assert.deepEqual(
    f.events.filter((event) => event.kind === 'sent'),
    [{ kind: 'sent', target: otherTargetId }],
  );
  assert.equal(f.state().busy, false);
});

const revocations: readonly { name: string; revoke(f: ReturnType<typeof fixture>): void }[] = [
  { name: 'logout', revoke: (f) => f.owner(null) },
  { name: 'identity change', revoke: (f) => f.owner({ ...ownerA, identityId: 'bruno' }) },
  { name: 'device change', revoke: (f) => f.owner({ ...ownerA, deviceId: 'phone_b' }) },
  { name: 'demo mode change', revoke: (f) => f.owner({ ...ownerA, mode: 'demo' }) },
  { name: 'source membership loss', revoke: (f) => f.spaces([targetId, otherTargetId]) },
  { name: 'target membership loss', revoke: (f) => f.spaces([sourceId, otherTargetId]) },
];

for (const { name, revoke } of revocations) {
  test(`${name} during preflight invalidates at refresh and cannot send after access returns`, async () => {
    const f = fixture();
    const canonical = deferred<TimelineEntry>();
    f.read(() => canonical.promise);
    const sending = f.session.send(targetId);
    revoke(f);
    f.session.refresh();
    assert.deepEqual(f.state(), { busy: false, error: null });
    f.owner({ ...ownerA });
    f.spaces([sourceId, targetId, otherTargetId]);
    f.session.refresh();
    const afterRefresh = [...f.events];
    canonical.resolve(message());
    await sending;
    assert.deepEqual(f.forwards, []);
    assert.deepEqual(f.events, afterRefresh);
  });

  test(`${name} after command acceptance suppresses late success without cancelling the command`, async () => {
    const f = fixture();
    const accepted = deferred<void>();
    f.forward(() => accepted.promise);
    const sending = f.session.send(targetId);
    await f.entered.promise;
    revoke(f);
    f.session.refresh();
    assert.deepEqual(f.state(), { busy: false, error: null });
    const afterRefresh = [...f.events];
    accepted.resolve();
    await sending;
    assert.equal(f.forwards.length, 1);
    assert.deepEqual(f.events, afterRefresh);
  });
}

for (const [name, canonical] of [
  ['deleted', message({ deleted: true })],
  ['system', message({ kind: { System: { text: 'Synthetic system entry' } } })],
  ['wrong message', message({ id: 'different_message' })],
] as const) {
  test(`${name} canonical source is unavailable and a later valid source can retry`, async () => {
    const f = fixture();
    f.read(async () => canonical);
    await f.session.send(targetId);
    assert.equal(f.forwards.length, 0);
    assert.ok(f.state().error instanceof ForwardMessageUnavailable);
    assert.equal(f.state().busy, false);
    f.read(async () => message());
    await f.session.send(targetId);
    assert.deepEqual(f.state(), { busy: false, error: null });
    assert.equal(f.forwards.length, 1);
    assert.deepEqual(
      f.events.filter((event) => event.kind === 'sent'),
      [{ kind: 'sent', target: targetId }],
    );
  });
}

for (const stage of ['lookup', 'command'] as const) {
  test(`a current ${stage} failure remains visible and retry succeeds without duplicate navigation`, async () => {
    const f = fixture();
    const failure = new Error(`Synthetic ${stage} failure`);
    if (stage === 'lookup')
      f.read(async () => {
        throw failure;
      });
    else
      f.forward(async () => {
        throw failure;
      });
    await f.session.send(targetId);
    assert.deepEqual(f.state(), { busy: false, error: failure });
    assert.equal(
      f.events.some((event) => event.kind === 'sent'),
      false,
    );
    f.read(async () => message());
    f.forward(async () => undefined);
    await f.session.send(targetId);
    assert.deepEqual(f.state(), { busy: false, error: null });
    assert.deepEqual(
      f.events.filter((event) => event.kind === 'sent'),
      [{ kind: 'sent', target: targetId }],
    );
  });
}

test('late lookup and command failures cannot replace UI state after leaving', async () => {
  for (const stage of ['lookup', 'command'] as const) {
    const f = fixture();
    const pending = deferred<TimelineEntry>();
    if (stage === 'lookup') f.read(() => pending.promise);
    else f.forward(() => pending.promise);
    const sending = f.session.send(targetId);
    if (stage === 'command') await f.entered.promise;
    f.session.blur();
    const afterLeaving = [...f.events];
    pending.reject(new Error('Synthetic late failure'));
    await sending;
    assert.deepEqual(f.events, afterLeaving);
  }
});

test('a missing captured owner or inaccessible destination never reaches the forwarding command', async () => {
  const unavailable = fixture('none');
  await unavailable.session.send(targetId);
  assert.deepEqual(unavailable.reads, []);
  assert.deepEqual(unavailable.forwards, []);
  const f = fixture();
  f.spaces([sourceId]);
  await f.session.send(targetId);
  assert.deepEqual(f.forwards, []);
});

test('destination selection only reviews canonical content; explicit confirmation alone forwards', async () => {
  const f = fixture();
  const canonical = message({
    forwarded: true,
    author: { id: 'source-author', name: 'Source author' } as TimelineEntry['author'],
  });
  f.read(async () => canonical);
  await f.session.prepare(targetId);
  const review = f.review()!;
  assert.equal(review.targetId, targetId);
  assert.deepEqual(review.message, canonical);
  assert.notEqual(review.message, canonical);
  assert.equal(f.forwards.length, 0);
  f.session.cancelReview();
  await f.session.confirm(review);
  await f.session.prepare(otherTargetId);
  await f.session.confirm(review);
  assert.equal(f.forwards.length, 0);
  await f.session.confirm(f.review()!);
  assert.deepEqual(f.forwards, [{ space: sourceId, message: messageId, target: otherTargetId }]);
});

for (const patch of [
  { kind: { Message: { text: 'Changed text', card: null } } },
  { voice: { item_id: 'voice', sha256: 'changed' } },
  { media: { presentation: 'changed', group: null } },
  { forwarded: true },
  { deleted: true },
])
  test(`content change ${Object.keys(patch)} requires a new review`, async () => {
    const f = fixture();
    await f.session.prepare(targetId);
    const review = f.review()!;
    f.read(async () => message(patch as Partial<TimelineEntry>));
    await f.session.confirm(review);
    assert.equal(f.forwards.length, 0);
    assert.ok(f.state().error instanceof ForwardMessageUnavailable);
    await f.session.confirm(review);
    assert.equal(f.forwards.length, 0);
  });

for (const revoke of ['epoch', 'writable', 'membership', 'blur'] as const)
  test(`review cannot cross ${revoke} changes`, async () => {
    const f = fixture();
    await f.session.prepare(targetId);
    const review = f.review()!;
    if (revoke === 'epoch') f.epoch(2);
    if (revoke === 'writable') f.writable(false);
    if (revoke === 'membership') f.spaces([sourceId]);
    if (revoke === 'blur') f.session.blur();
    f.session.refresh();
    assert.equal(f.review(), null);
    await f.session.confirm(review);
    assert.equal(f.forwards.length, 0);
  });

test('late review response cannot reopen after leaving and duplicate Send confirms once', async () => {
  const f = fixture();
  const pending = deferred<TimelineEntry>();
  f.read(() => pending.promise);
  const preparing = f.session.prepare(targetId);
  f.session.blur();
  pending.resolve(message());
  await preparing;
  assert.equal(f.review(), null);
  f.session.focus();
  f.read(async () => message());
  await f.session.prepare(targetId);
  const review = f.review()!;
  const ack = deferred<void>();
  f.forward(() => ack.promise);
  const sending = f.session.confirm(review);
  await f.entered.promise;
  await f.session.confirm(review);
  assert.equal(f.forwards.length, 1);
  ack.resolve();
  await sending;
  await f.session.confirm(review);
  assert.equal(f.forwards.length, 1);
});

test('send failure retains review and source projection for an explicit retry', async () => {
  const f = fixture();
  const source = message();
  f.read(async () => source);
  await f.session.prepare(targetId);
  const review = f.review()!;
  f.forward(async () => {
    throw Error('Not accepted');
  });
  await f.session.confirm(review);
  assert.equal(f.review(), review);
  assert.equal(f.state().busy, false);
  assert.equal(f.events.filter((event) => event.kind === 'sent').length, 0);
  assert.deepEqual(source, message());
  f.forward(async () => undefined);
  await f.session.confirm(review);
  assert.equal(f.events.filter((event) => event.kind === 'sent').length, 1);
});

test('forwarding to the source conversation still requires review and keeps the source untouched', async () => {
  const f = fixture();
  const source = message();
  f.read(async () => source);
  await f.session.prepare(sourceId);
  assert.equal(f.forwards.length, 0);
  await f.session.confirm(f.review()!);
  assert.deepEqual(f.forwards, [{ space: sourceId, message: messageId, target: sourceId }]);
  assert.deepEqual(source, message());
});
