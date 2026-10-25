import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  additionContext,
  additionFixture,
  peer,
  scope,
  settle,
} from './member-addition.fixtures.ts';
import { deferred } from './community-members.fixtures.ts';
import { createMemberAdditionPort } from '../communities/member-context-port.ts';
import { ClientStore } from './store.ts';
import { Driver, draftSession } from './store.fixtures.ts';

test('native addition port binds target, actor, device and observed cut on the normal Core command', async () => {
  const driver = new Driver(),
    snapshot = draftSession();
  driver.opening = async () => snapshot;
  const calls: unknown[] = [];
  driver.operation = async (command) => {
    calls.push(structuredClone(command));
    return command.op === 'snapshot' ? snapshot : additionContext();
  };
  const store = new ClientStore(driver);
  await store.open('account');
  const port = createMemberAdditionPort(store, 'sp_fixture'),
    owner = port.scope()!;
  await port.add(additionContext(), owner);
  assert.deepEqual(
    calls.find((c: any) => c.op === 'add_member_reviewed'),
    {
      op: 'add_member_reviewed',
      space_id: 'sp_fixture',
      identity_id: peer,
      expected_identity_id: owner.identity,
      expected_device_id: owner.device,
      observed_revision: additionContext().revision,
    },
  );
  const stale = { ...owner, epoch: owner.epoch + 1 };
  const before = calls.length;
  assert.throws(() => port.find('participant', stale));
  assert.throws(() => port.add(additionContext(), stale));
  assert.equal(calls.length, before);
});

test('confirmation decline and failed inclusion retain the query without automatic mutation retry', async () => {
  const f = await additionFixture();
  await f.open();
  f.session.search('participant', f.session.ticket());
  await f.session.refresh();
  f.confirm(async () => false);
  await f.select();
  assert.equal(f.writes.length, 0);
  f.confirm(async () => true);
  f.write(async () => {
    throw Error('join refused');
  });
  await f.select();
  assert.equal(f.writes.length, 1);
  assert.equal(f.session.getSnapshot().failed, true);
  assert.equal(f.session.getSnapshot().query, 'participant');
  await f.session.refresh();
  assert.equal(f.writes.length, 1);
  f.session.retire();
});

for (const replacement of [null, { ...scope, device: 'new-device' }, { ...scope, epoch: 2 }])
  test(`pending confirmation cannot add under changed scope ${JSON.stringify(replacement)}`, async () => {
    const f = await additionFixture();
    await f.open();
    const confirmation = deferred<boolean>();
    f.confirm(() => confirmation.promise);
    const selection = f.select();
    await settle();
    f.scope(replacement);
    f.session.activate(replacement);
    confirmation.resolve(true);
    await selection;
    assert.equal(f.writes.length, 0);
  });

test('role loss, pending membership and already-current member invalidate the exact explicit review', async () => {
  for (const patch of [
    { can_manage_members: false, can_add: false },
    { state: 'Updating' as const, can_add: false },
    {
      state: 'Active' as const,
      can_add: false,
      member: additionContext().candidate,
      role: 'member',
    },
    { revision: '2'.repeat(64) },
  ]) {
    const f = await additionFixture();
    await f.open();
    f.confirm(async () => {
      f.value({ ...additionContext(), ...patch });
      return true;
    });
    await f.select();
    assert.equal(f.writes.length, 0);
    f.session.retire();
  }
});

test('one in-flight inclusion is not reissued by canonical refresh, same-account reopen or duplicate tap', async () => {
  const f = await additionFixture();
  await f.open();
  const pending = deferred<ReturnType<typeof additionContext>>();
  f.write(() => pending.promise);
  const action = f.select();
  await settle();
  await f.select();
  await f.session.refresh();
  f.session.activate(scope);
  await settle();
  await f.open();
  await f.select();
  assert.equal(f.writes.length, 1);
  pending.resolve({ ...additionContext(), can_add: false, state: 'Updating' });
  await action;
  assert.equal(f.session.getSnapshot().busy, false);
  f.session.retire();
});

test('late search result cannot replace a newer query or resurrect a dismissed/account-retired sheet', async () => {
  const f = await additionFixture();
  await f.open();
  const pending = deferred<ReturnType<typeof additionContext>[]>();
  f.find(() => pending.promise);
  const refreshing = f.session.refresh();
  await settle();
  f.session.search('new query', f.session.ticket());
  f.session.close(f.session.ticket());
  pending.resolve([additionContext()]);
  await refreshing;
  assert.equal(f.session.getSnapshot().open, false);
  assert.deepEqual(f.session.getSnapshot().results, []);
  f.session.retire();
});
