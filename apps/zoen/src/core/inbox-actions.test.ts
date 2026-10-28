import assert from 'node:assert/strict';
import { test } from 'node:test';
import { inboxFixture, deferred, scope } from './inbox-actions.fixtures.ts';

for (const action of ['archive', 'pin'] as const)
  test(`${action} changes one canonical field then undoes by original receipt/revision while preserving timed mute`, async () => {
    const f = inboxFixture();
    await f.run(action);
    const receipt = f.session.getSnapshot().undo!.receipt;
    assert.equal(f.chat[action === 'archive' ? 'archived' : 'pinned'], true);
    assert.equal(f.chat.muted_until_ms, '1900000000000');
    await f.undo();
    assert.equal(f.chat[action === 'archive' ? 'archived' : 'pinned'], false);
    assert.equal(f.chat.muted_until_ms, '1900000000000');
    const undo = f.calls.find((c) => c.kind === 'undo')!;
    assert.equal(undo.kind, 'undo');
    if (undo.kind === 'undo') {
      assert.equal(undo.original_operation_id, receipt.operation_id);
      assert.equal(undo.observed_revision, receipt.revision);
      assert.notEqual(undo.operation_id, receipt.operation_id);
    }
    assert.equal(f.session.getSnapshot().undo, null);
  });

test('a lost change ACK reconciles the exact operation ID without another mutation', async () => {
  const f = inboxFixture();
  f.layout(async (input) => {
    const result = await f.canonical(input);
    if (input.kind === 'change') throw Error('Response lost after commit');
    return result;
  });
  await f.run('archive');
  const changes = f.calls.filter((c) => c.kind === 'change');
  const reconciles = f.calls.filter((c) => c.kind === 'reconcile');
  assert.equal(changes.length, 1);
  assert.equal(reconciles.length, 1);
  if (changes[0]!.kind === 'change' && reconciles[0]!.kind === 'reconcile')
    assert.equal(changes[0]!.operation_id, reconciles[0]!.operation_id);
  assert.ok(f.session.getSnapshot().undo);
  assert.equal(f.session.getSnapshot().error, null);
});

test('an unconfirmed change never advertises Undo or reapplies the intent', async () => {
  const f = inboxFixture();
  f.layout(async (input) => {
    if (input.kind === 'change') throw Error('Uncertain response');
    return f.canonical(input);
  });
  await f.run('archive');
  assert.equal(f.session.getSnapshot().undo, null);
  assert.equal(f.session.getSnapshot().error, 'change');
  assert.equal(f.calls.filter((c) => c.kind === 'change').length, 1);
});

test('a confirmed unchanged action does not manufacture an Undo receipt', async () => {
  const f = inboxFixture();
  f.layout(async (input) =>
    input.kind === 'change'
      ? { current: { ...f.current(), archived: true }, receipt: null }
      : f.canonical(input),
  );
  await f.run('archive');
  assert.equal(f.session.getSnapshot().undo, null);
  assert.equal(f.session.getSnapshot().error, null);
});

test('Undo conflict reconciles but cannot overwrite a newer same-field revision', async () => {
  const f = inboxFixture();
  await f.run('archive');
  f.setCurrent({ archive_revision: 'another-actor-event' });
  await f.undo();
  assert.equal(f.chat.archived, true);
  assert.equal(f.session.getSnapshot().error, 'undo');
  assert.equal(f.session.getSnapshot().undo, null);
});

test('a lost Undo ACK reconciles its new ID once and does not undo again', async () => {
  const f = inboxFixture();
  await f.run('archive');
  f.layout(async (input) => {
    const result = await f.canonical(input);
    if (input.kind === 'undo') throw Error('Undo response lost');
    return result;
  });
  await f.undo();
  assert.equal(f.chat.archived, false);
  assert.equal(f.calls.filter((c) => c.kind === 'undo').length, 1);
  assert.equal(f.session.getSnapshot().undo, null);
  assert.equal(f.session.getSnapshot().error, null);
});

for (const transition of ['retire', 'epoch', 'device', 'relay', 'membership'] as const)
  test(`a late preference review cannot dispatch after ${transition}`, async () => {
    const f = inboxFixture(),
      read = deferred<any>();
    f.layout((input) => (input.kind === 'read' ? read.promise : f.canonical(input)));
    const action = f.run('archive');
    await Promise.resolve();
    await Promise.resolve();
    if (transition === 'retire') f.session.retire();
    if (transition === 'epoch') f.owner({ ...scope, epoch: scope.epoch + 1 });
    if (transition === 'device') f.owner({ ...scope, device: 'other-device' });
    if (transition === 'relay') f.owner({ ...scope, relay: 'https://other-relay.test' });
    if (transition === 'membership') f.chat.members = [];
    read.resolve({ current: f.current(), receipt: null });
    await action;
    assert.equal(f.calls.filter((c) => c.kind === 'change').length, 0);
  });

test('duplicate queued callbacks are consumed once and cannot become a fresh action after ACK', async () => {
  const f = inboxFixture(),
    ticket = f.session.ticket(),
    observed = structuredClone(f.chat);
  const first = f.session.run(observed, 'archive', ticket);
  const second = f.session.run(observed, 'archive', ticket);
  await Promise.all([first, second]);
  await f.session.run(observed, 'archive', ticket);
  assert.equal(f.calls.filter((c) => c.kind === 'change').length, 1);
});

test('retirement leaves an admitted Core action intact but suppresses late feedback into the next scope', async () => {
  const f = inboxFixture(),
    ack = deferred<void>();
  let admitted = false;
  f.layout(async (input) => {
    const result = await f.canonical(input);
    if (input.kind === 'change') {
      admitted = true;
      await ack.promise;
    }
    return result;
  });
  const action = f.run('archive');
  for (let n = 0; n < 10; ++n) await Promise.resolve();
  assert.equal(admitted, true);
  assert.equal(f.chat.archived, true);
  f.session.retire();
  f.session.activate(scope);
  ack.resolve();
  await action;
  assert.equal(f.chat.archived, true);
  assert.equal(f.session.getSnapshot().undo, null);
  assert.equal(f.session.getSnapshot().pending, null);
});

test('unmute preserves its observed finite deadline and scoped read uses a frozen canonical target', async () => {
  const f = inboxFixture();
  await f.run('mute');
  const mute = f.calls.find((c) => c.kind === 'mute')!;
  assert.deepEqual(mute, { kind: 'mute', id: f.chat.id, value: false, observed: '1900000000000' });
  await f.run('read');
  assert.equal(f.chat.unread, 0);
  assert.equal(f.chat.muted_until_ms, null);
  const read = f.calls.find((call) => call.kind === 'mark_read');
  assert.ok(read && read.kind === 'mark_read');
  assert.equal(read.through_seq, '10');
  assert.equal(read.observed_manual_revision, '0');
});

test('read and mute preserve a prior authorized archive Undo; Undo preserves the newer mute state', async () => {
  const f = inboxFixture();
  await f.run('archive');
  const original = f.session.getSnapshot().undo!;
  await f.run('read');
  await f.run('mute');
  assert.equal(f.session.getSnapshot().undo, original);
  assert.equal(f.chat.muted_until_ms, null);
  await f.undo();
  assert.equal(f.chat.archived, false);
  assert.equal(f.chat.muted_until_ms, null);
});
