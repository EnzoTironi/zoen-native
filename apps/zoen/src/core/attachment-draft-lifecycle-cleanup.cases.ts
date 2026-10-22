import assert from 'node:assert/strict';
import { test } from 'node:test';
import { type AttachmentAsset, type AttachmentReceipt } from '../media/attachment-draft.ts';
import { asset, deferred, fixture, kind, receipt, scope } from './attachment-draft.fixtures.ts';

test('close drains a delayed picker and disposes its late selection without publishing it', async () => {
  const f = fixture();
  const picked = deferred<readonly AttachmentAsset[]>();
  const picking = f.draft.pick(() => picked.promise);
  await Promise.resolve();
  let publications = 0;
  f.draft.subscribe(() => publications++);
  const closing = f.draft.close();
  const countAtClose = publications;
  let finished = false;
  void closing.then(() => (finished = true));
  await Promise.resolve();
  assert.equal(finished, false);
  picked.resolve([asset(1)]);
  await picking;
  await closing;
  assert.deepEqual(f.discarded, [asset(1).uri]);
  assert.equal(f.draft.getSnapshot().phase, 'closed');
  assert.deepEqual(f.draft.getSnapshot().items, []);
  assert.equal(publications, countAtClose);
});

test('a temporary native picker interruption remains valid when the same route is focused on return', async () => {
  const f = fixture();
  const picked = deferred<readonly AttachmentAsset[]>();
  const picking = f.draft.pick(() => picked.promise);
  await Promise.resolve();
  f.setAvailable(false);
  f.setAvailable(true);
  picked.resolve([asset(1)]);
  await picking;
  assert.equal(f.draft.getSnapshot().items.length, 1);
  await f.draft.close();
});

test('picker results from a changed owner are disposed; no late error or send enters its UI', async () => {
  const f = fixture();
  const picked = deferred<readonly AttachmentAsset[]>();
  const picking = f.draft.pick(() => picked.promise);
  await Promise.resolve();
  let publications = 0;
  f.draft.subscribe(() => publications++);
  f.setAvailable(false);
  picked.resolve([asset(1)]);
  await picking;
  assert.equal(publications, 0);
  assert.deepEqual(f.discarded, [asset(1).uri]);
  assert.deepEqual(f.draft.getSnapshot().items, []);
  assert.equal((await f.draft.send()).current, false);
  assert.equal(f.calls.length, 0);
  await f.draft.close();
});

test('identity, device, mode and chat changes each fence a delayed picker', async () => {
  for (const next of [
    { ...scope, identityId: 'another-identity' },
    { ...scope, deviceId: 'another-device' },
    { ...scope, mode: 'demo' as const },
    { ...scope, spaceId: 'another-chat' },
  ]) {
    const f = fixture();
    const picked = deferred<readonly AttachmentAsset[]>();
    const picking = f.draft.pick(() => picked.promise);
    await Promise.resolve();
    f.setScope(next);
    picked.resolve([asset(1)]);
    await picking;
    assert.deepEqual(f.draft.getSnapshot().items, []);
    assert.deepEqual(f.discarded, [asset(1).uri]);
    assert.equal((await f.draft.send()).current, false);
    await f.draft.close();
  }
});

test('a late picker failure after close is consumed without exposing an old-owner error', async () => {
  const f = fixture();
  const picked = deferred<readonly AttachmentAsset[]>();
  const picking = f.draft.pick(() => picked.promise);
  await Promise.resolve();
  const closing = f.draft.close();
  picked.reject(new Error('old account picker failure'));
  await picking;
  await closing;
  assert.equal(f.draft.getSnapshot().error, null);
  assert.equal(f.draft.getSnapshot().phase, 'closed');
});

test('close waits for an admitted send, accepts its real result and prevents the next batch item', async () => {
  const f = fixture();
  await f.pick(asset(1), asset(2));
  const sent = deferred<AttachmentReceipt>();
  f.port.send = async (scope, item) => {
    f.calls.push({ scope, item });
    return sent.promise;
  };
  const sending = f.draft.send();
  await Promise.resolve();
  const closing = f.draft.close();
  assert.deepEqual(f.discarded, [], 'bytes remain leased while their native send is admitted');
  sent.resolve(receipt('intent-1'));
  const result = await sending;
  await closing;
  assert.deepEqual(result.accepted, [receipt('intent-1')]);
  assert.equal(result.current, false);
  assert.equal(result.complete, false);
  assert.equal(f.calls.length, 1);
  assert.deepEqual(f.discarded.sort(), [asset(1).uri, asset(2).uri].sort());
});

test('owner loss during native send preserves accepted delivery but suppresses late errors and later items', async () => {
  for (const accepted of [true, false]) {
    const f = fixture();
    await f.pick(asset(1), asset(2));
    const native = deferred<AttachmentReceipt>();
    f.port.send = async (scope, item) => {
      f.calls.push({ scope, item });
      return native.promise;
    };
    const sending = f.draft.send();
    await Promise.resolve();
    let publications = 0;
    f.draft.subscribe(() => publications++);
    f.setScope({ ...scope, identityId: 'replacement-account' });
    if (accepted) native.resolve(receipt('intent-1'));
    else native.reject(new Error('old account native error'));
    const result = await sending;
    assert.equal(result.current, false);
    assert.equal(result.complete, false);
    assert.equal(result.accepted.length, accepted ? 1 : 0);
    assert.equal(f.calls.length, 1);
    assert.equal(publications, 0);
    assert.equal(f.draft.getSnapshot().error, null);
    await f.draft.close();
    assert.deepEqual(f.discarded.sort(), [asset(1).uri, asset(2).uri].sort());
  }
});

test('cleanup failure never turns an accepted send into another mutation and close remains retryable', async () => {
  const f = fixture();
  await f.pick(asset(1));
  let fail = true;
  f.port.discard = async (uri) => {
    if (fail) throw new Error('synthetic removal failure');
    f.discarded.push(uri);
  };
  const result = await f.draft.send();
  assert.equal(result.complete, true);
  assert.equal(f.draft.getSnapshot().error?.kind, 'cleanup');
  assert.ok(f.draft.getSnapshot().items[0]?.accepted);
  await f.draft.send();
  assert.equal(f.calls.length, 1);
  await assert.rejects(f.draft.close(), kind('cleanup'));
  assert.equal(f.draft.getSnapshot().phase, 'closing');
  fail = false;
  await f.draft.close();
  assert.equal(f.draft.getSnapshot().phase, 'closed');
  assert.deepEqual(f.discarded, [asset(1).uri]);
  assert.equal(f.calls.length, 1);
});

test('a removed file with failed cleanup is retained for close retry instead of being forgotten', async () => {
  const f = fixture();
  await f.pick(asset(1), asset(2));
  let fail = true;
  f.port.discard = async (uri) => {
    if (uri === asset(1).uri && fail) throw new Error('synthetic cleanup failure');
    f.discarded.push(uri);
  };
  await assert.rejects(f.draft.remove('intent-1'), kind('cleanup'));
  assert.equal(f.draft.getSnapshot().items.length, 1);
  fail = false;
  await f.draft.close();
  assert.deepEqual(f.discarded.sort(), [asset(1).uri, asset(2).uri].sort());
});

test('a failed invalid-selection cleanup blocks new pickers until the retained files are disposed', async () => {
  const f = fixture();
  let fail = true;
  f.port.discard = async (uri) => {
    if (fail) throw new Error('synthetic cleanup failure');
    f.discarded.push(uri);
  };
  await assert.rejects(f.pick(asset(1, NaN)), kind('cleanup'));
  assert.deepEqual(f.draft.getSnapshot().items, []);
  let nativePickers = 0;
  await assert.rejects(
    f.draft.pick(async () => {
      nativePickers++;
      return [asset(2)];
    }),
    kind('cleanup'),
  );
  assert.equal(nativePickers, 0, 'invisible retained files cannot accumulate another batch');
  fail = false;
  await f.draft.close();
  assert.deepEqual(f.discarded, [asset(1).uri]);
});

test('removal cleanup debt cannot be bypassed by editing another caption or starting a new picker', async () => {
  const f = fixture();
  await f.pick(asset(1), asset(2));
  const normal = f.port.discard;
  f.port.discard = async (uri) => {
    if (uri === asset(1).uri) throw new Error('synthetic removal failure');
    await normal(uri);
  };
  await assert.rejects(f.draft.remove('intent-1'), kind('cleanup'));
  assert.equal(f.draft.setCaption('intent-2', 'a valid new caption'), true);
  assert.equal(f.draft.getSnapshot().error, null);
  let nativePickers = 0;
  await assert.rejects(
    f.draft.pick(async () => {
      nativePickers++;
      return [asset(3)];
    }),
    kind('cleanup'),
  );
  assert.equal(nativePickers, 0);
  f.port.discard = normal;
  await f.draft.close();
  assert.deepEqual(f.discarded.sort(), [asset(1).uri, asset(2).uri].sort());
});

test('external picker and send calls may synchronously request close without escaping the drain', async () => {
  const f = fixture();
  const picked = deferred<readonly AttachmentAsset[]>();
  let close!: Promise<void>;
  const picking = f.draft.pick(() => {
    close = f.draft.close();
    return picked.promise;
  });
  await Promise.resolve();
  picked.resolve([asset(1)]);
  await picking;
  await close;
  assert.deepEqual(f.discarded, [asset(1).uri]);

  const g = fixture();
  await g.pick(asset(2));
  const sent = deferred<AttachmentReceipt>();
  g.port.send = () => {
    close = g.draft.close();
    return sent.promise;
  };
  const sending = g.draft.send();
  await Promise.resolve();
  assert.deepEqual(g.discarded, []);
  sent.resolve(receipt('intent-1'));
  assert.equal((await sending).accepted.length, 1);
  await close;
  assert.deepEqual(g.discarded, [asset(2).uri]);
});

test('new native work cannot start after a publication synchronously closes the owner', async () => {
  const f = fixture();
  let picking = 0;
  f.draft.subscribe(() => {
    if (f.draft.getSnapshot().phase === 'picking') void f.draft.close();
  });
  await f.draft.pick(async () => {
    picking++;
    return [asset(1)];
  });
  await f.draft.close();
  assert.equal(picking, 0);

  const g = fixture();
  await g.pick(asset(2));
  g.draft.subscribe(() => {
    if (g.draft.getSnapshot().phase === 'sending') void g.draft.close();
  });
  const result = await g.draft.send();
  await g.draft.close();
  assert.equal(result.current, false);
  assert.equal(g.calls.length, 0);
});

test('attempt publication rechecks ownership before calling the native send port', async () => {
  const f = fixture();
  await f.pick(asset(1));
  f.draft.subscribe(() => {
    if (f.draft.getSnapshot().items[0]?.attempted) void f.draft.close();
  });
  const result = await f.draft.send();
  await f.draft.close();
  assert.equal(f.calls.length, 0);
  assert.deepEqual(result.accepted, []);
  assert.equal(result.current, false);
  assert.deepEqual(f.discarded, [asset(1).uri]);
});

test('async producer issuance drains and drops a canceled selection before close completes', async () => {
  const f = fixture();
  const issuing = deferred<string>();
  const released: string[] = [];
  f.port.mutationId = () => issuing.promise;
  f.port.releaseIntent = async (id, accepted) => {
    released.push(`${id}:${accepted}`);
  };
  const picking = f.pick(asset(1));
  await new Promise<void>((resolve) => setImmediate(resolve));
  let closed = false;
  const closing = f.draft.close().then(() => {
    closed = true;
  });
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.equal(closed, false);
  issuing.resolve('producer_1');
  await Promise.all([picking, closing]);
  assert.deepEqual(released, ['producer_1:false']);
  assert.equal(f.calls.length, 0);
  assert.equal(f.draft.getSnapshot().phase, 'closed');
});

test('UI receipt precedes producer ACK; lost ACK retries ACK without a second send', async () => {
  const f = fixture();
  await f.pick(asset(1));
  let acks = 0;
  f.port.releaseIntent = async (id, accepted) => {
    assert.equal(accepted, true);
    assert.equal(
      f.draft.getSnapshot().items.find((item) => item.mutationId === id)?.accepted?.itemId,
      `item-${id}`,
    );
    if (++acks === 1) throw new Error('lost ACK response');
  };
  assert.equal((await f.draft.send()).complete, true);
  assert.equal(f.draft.getSnapshot().error?.kind, 'cleanup');
  assert.equal(f.calls.length, 1);
  assert.equal(f.discarded.length, 0);
  assert.equal((await f.draft.send()).complete, true);
  assert.equal(f.calls.length, 1);
  assert.equal(acks, 2);
  assert.equal(f.discarded.length, 1);
  await f.draft.close();
});

test('explicit removal drops its issued sequence and cannot acknowledge an unsent file', async () => {
  const f = fixture();
  const released: string[] = [];
  f.port.releaseIntent = async (id, accepted) => {
    released.push(`${id}:${accepted}`);
  };
  await f.pick(asset(1), asset(2));
  await f.draft.remove(f.draft.getSnapshot().items[0]!.mutationId);
  await f.draft.close();
  assert.deepEqual(released.sort(), ['intent-1:false', 'intent-2:false']);
  assert.equal(f.calls.length, 0);
});
