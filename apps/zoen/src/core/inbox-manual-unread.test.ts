import assert from 'node:assert/strict';
import { test } from 'node:test';
import { inboxFixture, deferred, scope } from './inbox-actions.fixtures.ts';

test('manual unread reconciles the original signed operation ID and preserves existing layout Undo', async () => {
  const f = inboxFixture();
  await f.run('pin');
  const undo = f.session.getSnapshot().undo;
  f.chat.unread = 0;
  f.layout(async (input) => {
    const result = await f.canonical(input);
    if (input.kind === 'change' && input.field === 'manual_unread') throw Error('Lost ACK');
    return result;
  });
  await f.run('read');
  assert.equal(f.chat.manual_unread, true);
  assert.equal(f.chat.unread, 0);
  assert.equal(
    f.current().read_up_to_seq,
    '7',
    'Manual mark cannot retreat or advance actual reads',
  );
  assert.equal(f.session.getSnapshot().undo, undo);
  assert.equal(f.session.getSnapshot().error, null);
  const writes = f.calls.filter((c) => c.kind === 'change' && c.field === 'manual_unread');
  assert.equal(writes.length, 1);
  assert.ok(writes[0]!.kind === 'change');
  assert.equal(f.calls.find((c) => c.kind === 'reconcile')?.operation_id, writes[0]!.operation_id);
  await f.run('mute');
  await f.undo();
  assert.equal(f.chat.pinned, false);
  assert.equal(f.chat.manual_unread, true);
  assert.equal(f.chat.muted_until_ms, null);
});

test('lost read ACK reconciles the frozen watermark without swallowing later unread messages', async () => {
  const f = inboxFixture();
  f.layout(async (input) => {
    const result = await f.canonical(input);
    if (input.kind === 'mark_read') {
      f.chat.unread = 1;
      f.setCurrent({ read_through_seq: '11' });
      throw Error('ACK lost, then a newer message arrived');
    }
    return result;
  });
  await f.run('read');
  assert.equal(f.current().read_up_to_seq, '10');
  assert.equal(f.chat.unread, 1);
  assert.equal(f.session.getSnapshot().error, null);
  assert.equal(f.calls.filter((c) => c.kind === 'mark_read').length, 1);
});

test('a newer manual mark is not cleared by an older read intent or treated as its acknowledgement', async () => {
  const f = inboxFixture();
  f.layout(async (input) => {
    if (input.kind === 'mark_read') {
      f.chat.manual_unread = true;
      f.setCurrent({ manual_unread: true, manual_unread_revision: 'newer-mark' });
    }
    return f.canonical(input);
  });
  await f.run('read');
  assert.equal(f.chat.manual_unread, true);
  assert.equal(f.session.getSnapshot().error, 'change');
  assert.equal(f.calls.filter((c) => c.kind === 'mark_read').length, 1);
});

test('a delayed read context cannot admit manual-unread after retiring and reopening the same owner', async () => {
  const f = inboxFixture(),
    context = deferred<any>();
  f.chat.unread = 0;
  f.layout((input) => (input.kind === 'read' ? context.promise : f.canonical(input)));
  const pending = f.run('read');
  await Promise.resolve();
  await Promise.resolve();
  f.session.retire();
  f.session.activate(scope);
  context.resolve({ current: f.current(), receipt: null });
  await pending;
  assert.equal(f.calls.filter((c) => c.kind === 'change').length, 0);
  assert.equal(f.chat.manual_unread, undefined);
});
