import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fixture, main, deferred } from './normal-message-drafts.fixtures.ts';
import { captureReplyOptions, sharedReplySelection } from './reply-options.ts';
import { prepareSharedReply } from './shared-reply.ts';

const selection = {
  source_id: 'source',
  revision: 'signed',
  text: '😀 selected',
  spans: [{ start_utf16: 0, end_utf16: 11 }],
};
const quote = {
  space_id: 'private',
  message_id: 'source',
  revision: 'signed',
  author_id: 'author',
  author_name: 'Author',
  text: selection.text,
  selection,
};

test('reply options retain typing but permanently retire after reply or authority changes', async () => {
  const f = fixture();
  await f.drafts.activate(main);
  f.drafts.setReply(main, 'source', selection);
  const channel = { controller: f.drafts, scope: main };
  let allowed = true;
  const options = captureReplyOptions(channel, () => allowed);
  f.drafts.setText(main, 'Keep typing');
  assert.ok(options.current());
  allowed = false;
  assert.equal(options.current(), false);
  allowed = true;
  assert.equal(options.current(), false);
  const newer = captureReplyOptions(channel, () => true);
  f.drafts.setReply(main, 'other');
  f.drafts.setReply(main, 'source', selection);
  assert.equal(newer.current(), false);
  assert.equal(f.drafts.getSnapshot(main).text, 'Keep typing');
  options.retire();
  newer.retire();
});

test('moving a selection reads the exact durable source; stale routes cannot substitute it', async () => {
  const f = fixture();
  const save = f.port.save;
  f.port.save = async (...args) => ({ ...(await save(...args)), reply_quote: args[4] });
  await f.drafts.activate(main);
  f.drafts.setReply(main, 'source', selection);
  await f.drafts.flush(main);
  const channel = { controller: f.drafts, scope: main };
  const revision = f.drafts.getSnapshot(main).persistedRevision;
  assert.deepEqual(sharedReplySelection(channel, revision, 'source'), selection);
  assert.throws(() => sharedReplySelection(channel, 'older', 'source'));
  assert.throws(() => sharedReplySelection(channel, revision, 'other'));
  f.drafts.setReply(main, 'source', { ...selection, text: 'different' });
  assert.throws(
    () => sharedReplySelection(channel, revision, 'source'),
    'same source changed before autosave starts',
  );
  f.drafts.setReply(main, null);
  assert.throws(() => sharedReplySelection(channel, revision, 'source'));
  assert.equal(f.posts.length, 0);
});

test('cross-chat selection is copied, and delayed review cannot replace a newer destination reply', async () => {
  const f = fixture();
  await f.drafts.activate(main);
  const value = structuredClone(quote);
  f.drafts.setSharedReply(main, value);
  value.selection.spans[0]!.start_utf16 = 9;
  assert.equal(f.drafts.getSnapshot(main).sharedReply?.selection?.spans[0]?.start_utf16, 0);
  const review = deferred<typeof quote>();
  const preparing = prepareSharedReply({
    channel: { controller: f.drafts, scope: main },
    current: () => true,
    review: () => review.promise,
  });
  await Promise.resolve();
  f.drafts.setReply(main, 'newer');
  review.resolve(quote);
  await assert.rejects(preparing, /reply changed/);
  assert.equal(f.drafts.getSnapshot(main).replyTo, 'newer');
  assert.equal(f.posts.length, 0);
});
