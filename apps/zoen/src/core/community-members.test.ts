import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fixture, context, scope, peer, deferred, settle } from './community-members.fixtures.ts';
import { createMemberActionsPort } from '../communities/member-context-port.ts';
import type { ClientStore } from './store';

test('actual scoped port binds both current owner and the exact reviewed member revision', async () => {
  const calls: unknown[] = [];
  const state: any = {
    phase: 'ready',
    mode: 'account',
    erasePending: false,
    snapshot: {
      account: {
        identity_id: scope.identity,
        device_id: scope.device,
        relay_url: scope.relay,
        registered: true,
        unlocked: true,
        setup_pending: false,
      },
    },
  };
  const request = async (value: unknown) => {
    calls.push(value);
    return context();
  };
  const store = {
    sessionEpoch: 1,
    isSessionRetiring: false,
    getSnapshot: () => state,
    driver: { request },
    request,
  } as unknown as ClientStore;
  const port = createMemberActionsPort(store, 'sp_fixture');
  await port.read(peer, scope);
  await port.remove(context(), scope);
  assert.deepEqual(calls, [
    {
      op: 'space_member_context',
      space_id: 'sp_fixture',
      identity_id: peer,
      expected_identity_id: scope.identity,
      expected_device_id: scope.device,
    },
    {
      op: 'remove_member_reviewed',
      space_id: 'sp_fixture',
      identity_id: peer,
      expected_identity_id: scope.identity,
      expected_device_id: scope.device,
      observed_revision: '1'.repeat(64),
    },
  ]);
  state.snapshot.account.device_id = 'new-device';
  assert.throws(() => port.remove(context(), scope));
  assert.equal(calls.length, 2);
});

test('pending intent stays visible until Core reports confirmed absence', async () => {
  const f = fixture();
  await f.open();
  assert.equal(f.session.getSnapshot().context?.member?.name, 'Selected');
  assert.equal(f.session.canOpenProfile(f.session.ticket()), true);
  await f.remove();
  assert.equal(f.writes.length, 1);
  assert.equal(f.session.getSnapshot().context?.state, 'Removing');
  await f.remove();
  assert.equal(f.writes.length, 1);
  f.value({ ...context(), member: null, role: null, state: 'Removed', can_remove: false });
  await f.session.refresh(f.session.ticket());
  assert.equal(f.session.getSnapshot().selected, null);
});

for (const [key, value] of [
  ['identity', 'foreign'],
  ['device', 'foreign'],
  ['relay', 'https://other.invalid/'],
  ['epoch', 2],
] as const)
  test(`late confirmation cannot cross ${key}`, async () => {
    const f = fixture();
    await f.open();
    const prompt = deferred<boolean>();
    const task = f.remove(() => prompt.promise);
    f.scope({ ...scope, [key]: value });
    prompt.resolve(true);
    await task;
    assert.equal(f.writes.length, 0);
    assert.equal(f.session.ticket(), null);
  });

test('cancel and repeated prompt taps do not dispatch any removal', async () => {
  const f = fixture();
  await f.open();
  const prompt = deferred<boolean>();
  let prompts = 0;
  const task = f.remove(() => {
    ++prompts;
    return prompt.promise;
  });
  await f.remove(async () => {
    ++prompts;
    return true;
  });
  prompt.resolve(false);
  await task;
  assert.equal(prompts, 1);
  assert.equal(f.writes.length, 0);
  assert.equal(f.session.getSnapshot().busy, false);
});

test('blur and reopen invalidate the old dialog without disturbing a new selection', async () => {
  const f = fixture();
  await f.open();
  const prompt = deferred<boolean>();
  const task = f.remove(() => prompt.promise);
  f.session.retire();
  f.session.activate(scope);
  await f.open('c'.repeat(64));
  prompt.resolve(true);
  await task;
  assert.equal(f.writes.length, 0);
  assert.equal(f.session.getSnapshot().selected, 'c'.repeat(64));
});

for (const replacement of [
  { ...context(), can_remove: false },
  { ...context(), revision: '2'.repeat(64) },
  { ...context(), state: 'Updating' as const, can_remove: false },
])
  test(`current permission/rejoin revision/state ${replacement.revision.slice(0, 1)}/${replacement.state}/${replacement.can_remove} is rechecked after confirmation`, async () => {
    const f = fixture();
    await f.open();
    const prompt = deferred<boolean>();
    const task = f.remove(() => prompt.promise);
    f.value(replacement);
    prompt.resolve(true);
    await task;
    assert.equal(f.writes.length, 0);
    assert.deepEqual(f.session.getSnapshot().context, replacement);
  });

test('an ordinary canonical refresh with unchanged membership does not retire the review', async () => {
  const f = fixture();
  await f.open();
  const prompt = deferred<boolean>();
  const task = f.remove(() => prompt.promise);
  await f.session.refresh(f.session.ticket());
  prompt.resolve(true);
  await task;
  assert.equal(f.writes.length, 1);
});

test('canonical reads cannot clear the independent dispatched-write fence', async () => {
  const f = fixture();
  await f.open();
  const ack = deferred<ReturnType<typeof context>>();
  f.write(() => ack.promise);
  const task = f.remove();
  await settle();
  assert.equal(f.writes.length, 1);
  await f.session.refresh(f.session.ticket());
  assert.equal(f.session.getSnapshot().busy, true);
  await f.remove();
  assert.equal(f.writes.length, 1);
  ack.resolve({ ...context(), state: 'Removing', can_remove: false });
  await task;
  assert.equal(f.session.getSnapshot().busy, false);
});

test('persisted removal with lost response reconciles pending without resending', async () => {
  const f = fixture();
  await f.open();
  f.write(async (review) => {
    f.value({ ...review, state: 'Removing', can_remove: false });
    throw Error('response lost after enqueue');
  });
  await f.remove();
  assert.equal(f.writes.length, 1);
  assert.equal(f.session.getSnapshot().context?.state, 'Removing');
  assert.equal(f.session.getSnapshot().actionFailed, false);
  await f.remove();
  await f.session.refresh(f.session.ticket());
  assert.equal(f.writes.length, 1);
});

test('post-enqueue context failure stays unresolved and read retry never invokes mutation', async () => {
  const f = fixture();
  await f.open();
  f.write(async () => {
    f.read(async () => {
      throw Error('context unavailable');
    });
    throw Error('unknown ACK');
  });
  await f.remove();
  assert.equal(f.session.getSnapshot().phase, 'failed');
  assert.equal(f.writes.length, 1);
  f.read(async () => ({ ...context(), state: 'Removing', can_remove: false }));
  await f.session.refresh(f.session.ticket());
  await f.remove();
  assert.equal(f.writes.length, 1);
  assert.equal(f.session.getSnapshot().context?.state, 'Removing');
});

test('lost response after canonical removal closes only after the Core absence read', async () => {
  const f = fixture();
  await f.open();
  f.write(async () => {
    f.value({ ...context(), state: 'Removed', can_remove: false, member: null, role: null });
    throw Error('ACK lost');
  });
  await f.remove();
  assert.equal(f.writes.length, 1);
  assert.equal(f.session.getSnapshot().selected, null);
});

test('old write completion releases a reopened selected surface without applying the old target error', async () => {
  const f = fixture();
  await f.open();
  const ack = deferred<ReturnType<typeof context>>();
  f.write(() => ack.promise);
  const task = f.remove();
  await settle();
  f.session.close();
  await f.open('c'.repeat(64));
  assert.equal(f.session.getSnapshot().busy, true);
  ack.reject(Error('old target unavailable'));
  await task;
  assert.equal(f.writes.length, 1);
  assert.equal(f.session.getSnapshot().selected, 'c'.repeat(64));
  assert.equal(f.session.getSnapshot().busy, false);
  assert.equal(f.session.getSnapshot().actionFailed, false);
});

for (const patch of [
  { identity_id: 'foreign' },
  { device_id: 'foreign' },
  { member_id: 'foreign' },
  { space_id: 'foreign' },
  { revision: 'not-a-digest' },
  { state: 'invented' },
  { member: null, can_remove: true },
  { member: { ...context().member!, id: 'foreign' } },
])
  test(`foreign or malformed context ${Object.keys(patch).join(',')} is never actionable`, async () => {
    const f = fixture();
    f.read(async () => ({ ...context(), ...patch }) as any);
    await f.open();
    assert.equal(f.session.getSnapshot().phase, 'failed');
    assert.equal(f.session.canOpenProfile(f.session.ticket()), false);
    await f.remove();
    assert.equal(f.writes.length, 0);
  });

test('late read for a previous participant cannot replace the selected participant', async () => {
  const f = fixture();
  const old = deferred<ReturnType<typeof context>>();
  f.read((id) => (id === peer ? old.promise : Promise.resolve(context(id))));
  f.session.open(peer);
  await f.open('c'.repeat(64));
  old.resolve(context());
  await settle();
  assert.equal(f.session.getSnapshot().context?.member_id, 'c'.repeat(64));
});
