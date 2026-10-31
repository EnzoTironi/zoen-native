import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fixture, tick, type Element } from './durable-attachment-composer.fixtures.ts';
const poll = {
  kind: 'poll',
  reply_to: 'source',
  reply_intent: 'A',
  question: 'Keep this question',
  options: [
    { id: 'a', text: 'First' },
    { id: 'b', text: 'Second' },
  ],
};
const initial = () => ({
  revision: '1',
  content_json: JSON.stringify(poll),
  assets: [],
  attempted: false,
});
function nodes(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(nodes);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const element = value as Element;
  return [element, ...nodes(element.props.children)];
}
function button(tree: Element | null, name: string) {
  const found = nodes(tree).find((n) => n.props.label === name);
  assert.ok(found, name);
  return found.props.onPress as () => void;
}
test('a cold poll remains private in the composer; Resume opens one sheet with IDs only and blocks other media', async () => {
  const f = fixture(initial());
  f.render();
  await tick();
  const view = f.render();
  assert.equal(view.active, true);
  assert.equal(view.canAdd, false);
  assert.equal(view.busy, true);
  assert.equal(view.field, undefined);
  assert.ok(nodes(view.review).some((n) => n.props.testID === 'poll-draft-review'));
  const resume = button(view.review, 'attachmentTabs.resumePoll');
  resume();
  resume();
  assert.equal(f.navigations.length, 1);
  assert.deepEqual(JSON.parse(JSON.stringify(f.navigations[0])), {
    pathname: '/chat-attachment/[id]',
    params: {
      id: 'chat',
      kind: 'poll',
      thread: '',
      identity: 'owner',
      device: 'device',
      epoch: '7',
    },
  });
  assert.equal(f.sent.length, 0);
  assert.equal(f.writes.length, 0);
  f.unmount();
});
test('poll dock unlink preserves latest question and reports captured reply, while old controls refuse a suspended composer', async () => {
  const f = fixture(initial());
  f.render();
  await tick();
  const cancelled: unknown[] = [];
  const view = f.render({ onReplyCancelled: (reply) => cancelled.push(reply) });
  const context = nodes(view.review).find((n) => n.type === 'AttachmentReplyContext')!;
  const unlink = context.props.onCancel as () => void;
  await f.channel.save({ ...poll, kind: 'poll', question: 'Latest question' });
  unlink();
  await tick();
  assert.deepEqual(JSON.parse(JSON.stringify(cancelled)), [
    { reply_to: 'source', reply_intent: 'A' },
  ]);
  const value = f.channel.getSnapshot().content;
  assert.equal(value?.kind === 'poll' && value.question, 'Latest question');
  assert.equal(value?.reply_to, null);
  const resume = button(view.review, 'attachmentTabs.resumePoll');
  f.render({ suspended: true });
  resume();
  assert.equal(f.navigations.length, 0);
  assert.equal(f.sent.length, 0);
  f.unmount();
});
test('discarding a private poll never publishes and cannot erase another pending media kind from stale controls', async () => {
  const f = fixture(initial());
  f.render();
  await tick();
  const view = f.render();
  const discard = button(view.review, 'chat.cancel');
  discard();
  await tick();
  assert.equal(f.channel.getSnapshot().content, null);
  assert.equal(f.sent.length, 0);
  await f.channel.save({
    kind: 'poll',
    reply_to: null,
    question: 'Second private draft',
    options: poll.options,
  });
  f.render({ disabled: true });
  discard();
  await tick();
  assert.equal(f.channel.getSnapshot().content?.kind, 'poll');
  f.unmount();
});
