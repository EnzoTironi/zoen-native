import assert from 'node:assert/strict';
import { test } from 'node:test';
import { deferred, fixture, key, kind, main } from './normal-message-drafts.fixtures.ts';
import { type NormalDraftScope } from './normal-message-drafts.ts';

test('cold restore gates input, caches a loaded scope, and keeps opaque revisions and immutable scope', async () => {
  const f = fixture();
  const gate = deferred<void>();
  f.hooks.load = () => gate.promise;
  f.stored.set(key(main), {
    revision: 'native_token-A',
    text: 'Saved 🌤️\ntext',
    reply_to: 'quote',
  });
  const scope = { ...main, owner: { ...main.owner } };
  const loading = f.drafts.activate(scope);
  assert.equal(f.drafts.getSnapshot(scope).loading, true);
  assert.equal(f.drafts.setText(scope, 'Too early'), false);
  assert.equal(f.drafts.setReply(scope, 'Too early'), false);
  scope.owner.identityId = 'changed outside';
  gate.resolve();
  await loading;
  const state = f.drafts.getSnapshot(main);
  assert.equal(state.text, 'Saved 🌤️\ntext');
  assert.equal(state.replyTo, 'quote');
  assert.equal(state.persistedRevision, 'native_token-A');
  assert.equal(state.scope.owner.identityId, main.owner.identityId);
  assert.ok(Object.isFrozen(state.scope));
  assert.ok(Object.isFrozen(state.scope.owner));
  f.drafts.setText(main, 'New local text');
  await f.drafts.activate(main);
  await f.drafts.flush(main);
  assert.equal(f.drafts.getSnapshot(main).text, 'New local text');
  assert.equal(f.calls.filter((call) => call.op === 'load').length, 1);
});

test('empty recovery barrier never flushes or resumes an ambiguous normal send', async () => {
  const f = fixture();
  await f.drafts.activate(main);
  f.drafts.setText(main, 'An ambiguous accepted send');
  f.hooks.sent = async () => {
    throw new Error('Synthetic ACK loss');
  };
  await assert.rejects(f.drafts.send(main), kind('send'));
  const calls = f.calls.length;
  let recovered = false;
  await assert.rejects(
    f.drafts.withEmptyDraft(main, async () => {
      recovered = true;
    }),
    kind('conflict'),
  );
  assert.equal(recovered, false);
  assert.equal(f.calls.length, calls);
  assert.equal(f.posts.length, 1);
});

test('empty recovery barrier preserves concurrent typing and invalidates late adoption without a send', async () => {
  const f = fixture();
  const entered = deferred<void>(),
    done = deferred<void>();
  let current: (() => boolean) | undefined;
  const task = f.drafts.withEmptyDraft(main, async (revision, unchanged) => {
    assert.equal(revision, '0');
    current = unchanged;
    entered.resolve();
    await done.promise;
  });
  await entered.promise;
  assert.equal(current?.(), true);
  f.drafts.setText(main, 'Typed while preparing media');
  assert.equal(current?.(), false);
  done.resolve();
  await task;
  await f.drafts.flush(main);
  assert.equal(f.current(main).text, 'Typed while preparing media');
  assert.equal(f.posts.length, 0);
  assert.equal(
    f.calls.some((call) => call.op === 'send'),
    false,
  );
});

test('empty recovery barrier rejects occupied normal text or reply without mutation', async () => {
  for (const reply of [false, true]) {
    const f = fixture();
    await f.drafts.activate(main);
    if (reply) f.drafts.setReply(main, 'quoted');
    else f.drafts.setText(main, 'Keep it');
    await f.drafts.flush(main);
    const before = { ...f.current(main) },
      calls = f.calls.length;
    await assert.rejects(
      f.drafts.withEmptyDraft(main, async () => assert.fail('occupied')),
      kind('conflict'),
    );
    assert.deepEqual(f.current(main), before);
    assert.equal(f.calls.length, calls);
  }
});

test('account, device, session, demo, chat and two thread scopes cannot share drafts', async () => {
  const f = fixture();
  const scopes: NormalDraftScope[] = [
    main,
    { ...main, owner: { ...main.owner, identityId: 'another' } },
    { ...main, owner: { ...main.owner, deviceId: 'another' } },
    { ...main, owner: { ...main.owner, sessionEpoch: 2 } },
    { ...main, owner: { ...main.owner, mode: 'demo' } },
    { ...main, spaceId: 'another' },
    { ...main, threadRootId: 'thread-A' },
    { ...main, threadRootId: 'thread-B' },
  ];
  for (const [index, scope] of scopes.entries()) {
    await f.drafts.activate(scope);
    f.drafts.setText(scope, `Draft ${index}`);
    await f.drafts.flush(scope);
  }
  for (const [index, scope] of scopes.entries())
    assert.equal(f.drafts.getSnapshot(scope).text, `Draft ${index}`);
  assert.equal(f.stored.size, scopes.length);
});

test('same-turn typing coalesces and an admitted old save never overwrites newer text', async () => {
  const f = fixture();
  await f.drafts.activate(main);
  const entered = deferred<void>();
  const gate = deferred<void>();
  let saves = 0;
  f.hooks.save = async () => {
    if (++saves !== 1) return;
    entered.resolve();
    await gate.promise;
  };
  f.drafts.setText(main, 'A');
  f.drafts.setText(main, 'AB');
  await entered.promise;
  f.drafts.setText(main, 'ABC');
  assert.equal(f.drafts.getSnapshot(main).text, 'ABC');
  gate.resolve();
  await f.drafts.flush(main);
  assert.equal(f.current(main).text, 'ABC');
  assert.equal(f.drafts.getSnapshot(main).text, 'ABC');
  assert.equal(saves, 2);
});
