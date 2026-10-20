import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  AttachmentDraft,
  AttachmentDraftFailure,
  MAX_ATTACHMENT_ITEMS,
  MAX_ATTACHMENT_TOTAL_BYTES,
  type AttachmentAsset,
  type AttachmentDraftItem,
  type AttachmentDraftPort,
  type AttachmentDraftScope,
  type AttachmentReceipt,
} from '../media/attachment-draft.ts';
import { MAX_FILE_CAPTION_BYTES } from '../media/types.ts';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (cause: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}
const scope: AttachmentDraftScope = {
  mode: 'account',
  identityId: 'identity',
  deviceId: 'device',
  spaceId: 'chat',
  replyTo: null,
  thread: false,
};
const asset = (id: number, size = 100): AttachmentAsset => ({
  uri: `file:///owned/selection-${id}/qa.pdf`,
  name: `qa-${id}.pdf`,
  mime: 'application/pdf',
  size,
});
const receipt = (id: string): AttachmentReceipt => ({
  itemId: `item-${id}`,
  messageId: `msg-${id}`,
});
function fixture(capturedScope = scope) {
  const calls: { scope: AttachmentDraftScope; item: AttachmentDraftItem }[] = [];
  const discarded: string[] = [];
  let available = true;
  let liveScope = { ...capturedScope };
  let sequence = 0;
  const port: AttachmentDraftPort = {
    current: (captured) =>
      available &&
      captured.mode === liveScope.mode &&
      captured.identityId === liveScope.identityId &&
      captured.deviceId === liveScope.deviceId &&
      captured.spaceId === liveScope.spaceId,
    mutationId: () => `intent-${++sequence}`,
    send: async (scope, item) => {
      calls.push({ scope, item });
      return receipt(item.mutationId);
    },
    discard: async (uri) => {
      discarded.push(uri);
    },
  };
  const draft = new AttachmentDraft(capturedScope, port);
  return {
    draft,
    port,
    calls,
    discarded,
    setAvailable: (value: boolean) => {
      available = value;
    },
    setScope: (value: AttachmentDraftScope) => {
      liveScope = value;
    },
    pick: (...items: AttachmentAsset[]) => draft.pick(async () => items),
  };
}
const kind = (expected: string) => (error: unknown) =>
  error instanceof AttachmentDraftFailure && error.kind === expected;

test('attachment selection copies assets and owner scope, with independent captions and stable targets', async () => {
  const owner = { ...scope };
  const f = fixture(owner);
  owner.spaceId = 'foreign-chat';
  const first = { ...asset(1) };
  await f.pick(first, asset(2));
  first.name = 'later mutation.pdf';
  const before = f.draft.getSnapshot();
  assert.equal(before.scope.spaceId, 'chat');
  assert.equal(before.items[0]?.name, 'qa-1.pdf');
  assert.equal(f.draft.setCaption('intent-1', '  página 53\noriginal  '), true);
  assert.equal(f.draft.setCaption('intent-2', 'Página 161'), true);
  assert.equal(f.draft.retarget('root', true), true);
  assert.equal(f.draft.getSnapshot().scope.replyTo, 'root');
  assert.deepEqual(
    f.draft.getSnapshot().items.map(({ mutationId, caption }) => ({ mutationId, caption })),
    [
      { mutationId: 'intent-1', caption: '  página 53\noriginal  ' },
      { mutationId: 'intent-2', caption: 'Página 161' },
    ],
  );
  assert.equal(before.items[0]?.caption, '', 'older subscribed snapshots remain immutable');
  assert.ok(Object.isFrozen(before.items[0]));
  await f.draft.close();
});

test('ten files, verified zero size and the exact aggregate byte ceiling are admitted', async () => {
  const f = fixture();
  await f.pick(
    ...Array.from({ length: MAX_ATTACHMENT_ITEMS }, (_, index) =>
      asset(index, index === 0 ? MAX_ATTACHMENT_TOTAL_BYTES : 0),
    ),
  );
  assert.equal(f.draft.getSnapshot().items.length, 10);
  await assert.rejects(f.pick(asset(11, 0)), kind('limit'));
  assert.deepEqual(f.discarded, [asset(11).uri]);
  assert.equal(f.draft.getSnapshot().items.length, 10);
  await f.draft.close();

  const oversized = fixture();
  await assert.rejects(oversized.pick(asset(1, MAX_ATTACHMENT_TOTAL_BYTES + 1)), kind('size'));
  assert.deepEqual(oversized.draft.getSnapshot().items, []);
  assert.deepEqual(oversized.discarded, [asset(1).uri]);
  await oversized.draft.close();
});

test('invalid or oversized selection is rejected as a whole without discarding an existing selection', async () => {
  const invalid: AttachmentAsset[] = [
    asset(2, -1),
    asset(2, 0.5),
    asset(2, NaN),
    asset(2, Infinity),
    { ...asset(2), name: '../qa.pdf' },
    { ...asset(2), name: '😀'.repeat(257) },
    { ...asset(2), mime: 'text/plain\nheader' },
    asset(2, MAX_ATTACHMENT_TOTAL_BYTES),
  ];
  for (const next of invalid) {
    const f = fixture();
    await f.pick(asset(1));
    await assert.rejects(f.pick(next));
    assert.deepEqual(
      f.draft.getSnapshot().items.map((item) => item.uri),
      [asset(1).uri],
    );
    assert.deepEqual(f.discarded, [next.uri]);
    await f.draft.close();
  }
  const f = fixture();
  await f.pick(asset(1));
  await assert.rejects(f.pick(asset(1)), kind('invalid'));
  assert.deepEqual(f.discarded, [], 'a repeated URI does not release its already retained owner');
  await assert.rejects(f.pick(asset(2), asset(2)), kind('invalid'));
  assert.deepEqual(f.discarded, [asset(2).uri]);
  await f.draft.close();
});

test('a canceled picker preserves the existing batch and duplicate intent IDs never enter it', async () => {
  const f = fixture();
  await f.pick(asset(1));
  const previous = f.draft.getSnapshot().items;
  await f.pick();
  assert.deepEqual(f.draft.getSnapshot().items, previous);
  f.port.mutationId = () => 'intent-1';
  await assert.rejects(f.pick(asset(2)), kind('invalid'));
  assert.deepEqual(f.draft.getSnapshot().items, previous);
  assert.deepEqual(f.discarded, [asset(2).uri]);
  await f.draft.close();
});

test('caption bounds count UTF-8 and preserve exact nonblank text without accepting NUL', async () => {
  const f = fixture();
  await f.pick(asset(1));
  const caption = '😀'.repeat(MAX_FILE_CAPTION_BYTES / 4);
  assert.equal(f.draft.setCaption('intent-1', caption), true);
  assert.equal(f.draft.setCaption('intent-1', `${caption}a`), false);
  assert.equal(f.draft.getSnapshot().error?.kind, 'caption');
  assert.equal(f.draft.getSnapshot().items[0]?.caption, caption);
  assert.equal(f.draft.setCaption('intent-1', 'bad\0caption'), false);
  const exact = '  UTF-8: café 🇧🇷\nsecond line  ';
  assert.equal(f.draft.setCaption('intent-1', exact), true);
  await f.draft.send();
  assert.equal(f.calls[0]?.item.caption, exact);
  await f.draft.close();
});

test('removing a selection releases only that source, then the remaining file can be sent', async () => {
  const f = fixture();
  await f.pick(asset(1), asset(2));
  await f.draft.remove('intent-1');
  assert.deepEqual(f.discarded, [asset(1).uri]);
  assert.deepEqual(
    f.draft.getSnapshot().items.map((item) => item.mutationId),
    ['intent-2'],
  );
  await f.draft.send();
  assert.deepEqual(
    f.calls.map(({ item }) => item.mutationId),
    ['intent-2'],
  );
  await f.draft.close();
  assert.deepEqual(f.discarded, [asset(1).uri, asset(2).uri]);
});

test('partial acceptance retries only unresolved items with stable IDs, caption and reply scope', async () => {
  const f = fixture();
  await f.pick(asset(1), asset(2), asset(3));
  f.draft.setCaption('intent-1', 'first');
  f.draft.setCaption('intent-2', 'second');
  f.draft.retarget('thread-root', true);
  const normalSend = f.port.send;
  let fail = true;
  f.port.send = async (scope, item) => {
    if (item.mutationId === 'intent-2' && fail) {
      f.calls.push({ scope, item });
      throw new Error('temporary transport failure');
    }
    return normalSend(scope, item);
  };
  const first = await f.draft.send();
  assert.deepEqual(first.accepted, [receipt('intent-1')]);
  assert.equal(first.complete, false);
  assert.equal(f.draft.getSnapshot().error?.kind, 'send');
  assert.deepEqual(
    f.draft.getSnapshot().items.map((item) => [item.attempted, Boolean(item.accepted)]),
    [
      [true, true],
      [true, false],
      [false, false],
    ],
  );
  assert.equal(f.draft.setCaption('intent-2', 'changed after an uncertain send'), false);
  assert.equal(f.draft.setCaption('intent-3', 'not yet attempted'), true);
  assert.equal(f.draft.retarget('another-root', false), false);
  await f.draft.remove('intent-2');
  assert.equal(f.draft.getSnapshot().items.length, 3, 'uncertain delivery is retained for retry');
  fail = false;
  const second = await f.draft.send();
  assert.deepEqual(second.accepted, [receipt('intent-2'), receipt('intent-3')]);
  assert.equal(second.complete, true);
  assert.deepEqual(
    f.calls.map(({ item }) => item.mutationId),
    ['intent-1', 'intent-2', 'intent-2', 'intent-3'],
  );
  assert.ok(f.calls.every(({ scope }) => scope.replyTo === 'thread-root' && scope.thread));
  assert.equal(f.calls[1]?.item.caption, f.calls[2]?.item.caption);
  assert.deepEqual(await f.draft.send(), { accepted: [], complete: true, current: true });
  assert.equal(f.calls.length, 4, 'known accepted items are never submitted again');
  await f.draft.close();
});

test('a lost accepted response keeps one intent so retry resolves the same canonical message', async () => {
  const f = fixture();
  const canonical = new Map<string, AttachmentReceipt>();
  let lost = true;
  f.port.send = async (scope, item) => {
    f.calls.push({ scope, item });
    if (!canonical.has(item.mutationId)) canonical.set(item.mutationId, receipt(item.mutationId));
    if (lost) {
      lost = false;
      throw new Error('response lost after acceptance');
    }
    return canonical.get(item.mutationId)!;
  };
  await f.pick(asset(1));
  f.draft.setCaption('intent-1', 'signed caption');
  assert.equal((await f.draft.send()).complete, false);
  assert.equal(f.draft.setCaption('intent-1', 'different caption'), false);
  const retry = await f.draft.send();
  assert.equal(retry.complete, true);
  assert.deepEqual(retry.accepted, [receipt('intent-1')]);
  assert.equal(canonical.size, 1);
  assert.deepEqual(f.calls[0], f.calls[1]);
  await f.draft.close();
});

test('the captured target stays frozen after its only accepted row is removed', async () => {
  const f = fixture();
  await f.pick(asset(1), asset(2));
  f.draft.retarget('original-reply', false);
  const normal = f.port.send;
  f.port.send = async (scope, item) => {
    const result = await normal(scope, item);
    f.setAvailable(false);
    return result;
  };
  await f.draft.send();
  f.setAvailable(true);
  await f.draft.remove('intent-1');
  assert.equal(f.draft.getSnapshot().items.length, 1);
  assert.equal(f.draft.getSnapshot().items[0]?.attempted, false);
  assert.equal(f.draft.getSnapshot().targetFrozen, true);
  assert.equal(f.draft.retarget('different-reply', false), false);
  assert.equal(f.draft.getSnapshot().scope.replyTo, 'original-reply');
  await f.draft.close();
});

test('duplicate presses are latched before external send work and lock the target immediately', async () => {
  const f = fixture();
  await f.pick(asset(1));
  const result = deferred<AttachmentReceipt>();
  f.port.send = async (scope, item) => {
    f.calls.push({ scope, item });
    return result.promise;
  };
  const first = f.draft.send();
  const duplicate = f.draft.send();
  assert.equal(f.draft.retarget('new-target', false), false);
  assert.equal(f.draft.setCaption('intent-1', 'late caption'), false);
  assert.equal((await duplicate).complete, false);
  assert.equal(f.calls.length, 1);
  result.resolve(receipt('intent-1'));
  assert.equal((await first).complete, true);
  await f.draft.close();
});

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
