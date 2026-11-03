import assert from 'node:assert/strict';
import { test } from 'node:test';
import { NormalMessageDrafts } from './normal-message-drafts.ts';
import { fixture, main, deferred } from './normal-message-drafts.fixtures.ts';
import { prepareSharedReply } from './shared-reply.ts';
const quote = {
  space_id: 'source',
  message_id: 'source-message',
  revision: 'source-revision',
  author_id: 'sender',
  author_name: 'Sender',
  text: 'The selected message',
};
function sharedFixture() {
  const f = fixture();
  const original = f.port.save;
  f.port.save = async (scope, revision, text, reply, selection, shared) => {
    const saved = await original(scope, revision, text, reply, selection, shared);
    const full = { ...saved, shared_reply: shared ?? undefined };
    f.stored.set(JSON.stringify(scope), full);
    return full;
  };
  return f;
}
test('choosing a destination preserves its text, prepares no send, and survives controller recreation', async () => {
  const f = sharedFixture();
  await f.drafts.activate(main);
  f.drafts.setText(main, 'Already typing here');
  await prepareSharedReply({
    channel: { scope: main, controller: f.drafts },
    current: () => true,
    review: async () => quote,
  });
  assert.equal(f.current(main).text, 'Already typing here');
  assert.deepEqual(f.current(main).shared_reply, quote);
  assert.equal(f.posts.length, 0);
  const next = new NormalMessageDrafts(f.port);
  await next.activate(main);
  assert.deepEqual(next.getSnapshot(main).sharedReply, quote);
  next.setReply(main, null);
  await next.flush(main);
  assert.equal(f.current(main).text, 'Already typing here');
  assert.equal(f.current(main).shared_reply, undefined);
});
test('cancel or lost source authority during review cannot replace the destination draft', async () => {
  const f = sharedFixture();
  await f.drafts.activate(main);
  f.drafts.setText(main, 'Preserve');
  await f.drafts.flush(main);
  const reviewed = deferred<typeof quote>();
  let current = true;
  const preparing = prepareSharedReply({
    channel: { scope: main, controller: f.drafts },
    current: () => current,
    review: () => reviewed.promise,
  });
  await Promise.resolve();
  current = false;
  reviewed.resolve(quote);
  await assert.rejects(preparing);
  assert.equal(f.current(main).text, 'Preserve');
  assert.equal(f.current(main).shared_reply, undefined);
  assert.equal(f.posts.length, 0);
});
test('an uncertain send keeps its shared quote; successful retry clears exactly the consumed context', async () => {
  const f = sharedFixture();
  await f.drafts.activate(main);
  f.drafts.setText(main, 'Answer');
  assert.ok(f.drafts.setSharedReply(main, quote));
  await f.drafts.flush(main);
  f.hooks.sent = async () => {
    throw new Error('lost acknowledgement');
  };
  await assert.rejects(f.drafts.send(main));
  assert.deepEqual(f.drafts.getSnapshot(main).sharedReply, quote);
  assert.equal(f.posts.length, 1);
  assert.equal(f.drafts.setSharedReply(main, { ...quote, message_id: 'another' }), false);
  f.hooks.sent = undefined;
  await f.drafts.send(main);
  assert.equal(f.drafts.getSnapshot(main).sharedReply, null);
  assert.equal(f.posts.length, 1);
});
