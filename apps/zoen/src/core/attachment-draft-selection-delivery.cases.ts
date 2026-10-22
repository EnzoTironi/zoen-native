import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  MAX_ATTACHMENT_ITEMS,
  MAX_ATTACHMENT_TOTAL_BYTES,
  type AttachmentAsset,
  type AttachmentReceipt,
} from '../media/attachment-draft.ts';
import { MAX_FILE_CAPTION_BYTES } from '../media/types.ts';
import { asset, deferred, fixture, kind, receipt, scope } from './attachment-draft.fixtures.ts';

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
