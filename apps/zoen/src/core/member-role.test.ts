import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createMemberActionsPort } from '../communities/member-context-port.ts';
import type { ClientStore } from './store';
import {
  roleFixture,
  roleContext,
  reader,
  admin,
  context,
  peer,
  scope,
  deferred,
  settle,
} from './member-role.fixtures.ts';

test('role port writes the selected operation digest, never the removal context digest', async () => {
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
  const store = {
    sessionEpoch: 1,
    isSessionRetiring: false,
    getSnapshot: () => state,
    request: async (command: unknown) => {
      calls.push(command);
      return roleContext();
    },
  } as unknown as ClientStore;
  const port = createMemberActionsPort(store, 'sp_fixture');
  await port.setRole(roleContext(), reader, scope);
  assert.deepEqual(calls, [
    {
      op: 'set_member_role_reviewed',
      space_id: 'sp_fixture',
      identity_id: peer,
      expected_identity_id: scope.identity,
      expected_device_id: scope.device,
      observed_revision: reader.revision,
      role: 'reader',
    },
  ]);
  state.snapshot.account.device_id = 'foreign';
  assert.throws(() => port.setRole(roleContext(), admin, scope));
  assert.equal(calls.length, 1);
});

test('queued role keeps the canonical role and offers no second mutation until actual ACK', async () => {
  const f = await roleFixture();
  await f.setRole();
  assert.equal(f.roleWrites.length, 1);
  assert.equal(f.session.getSnapshot().context?.role, 'member');
  assert.equal(f.session.getSnapshot().context?.pending_role, 'reader');
  assert.equal(f.session.getSnapshot().context?.state, 'Updating');
  await f.setRole(admin);
  await f.remove();
  assert.equal(f.roleWrites.length, 1);
  assert.equal(f.writes.length, 0);
  f.value(
    roleContext({
      role: 'reader',
      pending_role: null,
      role_options: [{ role: 'member', revision: 'c'.repeat(64) }, admin],
      revision: '2'.repeat(64),
    }),
  );
  await f.session.refresh(f.session.ticket());
  assert.equal(f.session.getSnapshot().context?.role, 'reader');
  assert.equal(f.session.getSnapshot().actionFailed, false);
});

for (const option of [
  { ...reader, role: 'admin' },
  { ...reader, revision: 'f'.repeat(64) },
  { role: 'owner', revision: 'c'.repeat(64) },
])
  test(`unoffered role/operation ${option.role}/${option.revision[0]} cannot even open a review`, async () => {
    const f = await roleFixture();
    let prompts = 0;
    await f.setRole(option, async () => {
      ++prompts;
      return true;
    });
    assert.equal(prompts, 0);
    assert.equal(f.roleWrites.length, 0);
  });

for (const replacement of [
  roleContext({ revision: '2'.repeat(64) }),
  roleContext({ role_options: [admin] }),
  roleContext({ role_options: [{ ...reader, revision: 'c'.repeat(64) }] }),
  roleContext({ state: 'Updating', pending_role: 'admin', can_remove: false, role_options: [] }),
  roleContext({ role: 'owner', can_remove: false, role_options: [] }),
])
  test(`confirmation rereads the exact option and context ${replacement.state}/${replacement.role}/${replacement.revision[0]}`, async () => {
    const f = await roleFixture(),
      answer = deferred<boolean>();
    const task = f.setRole(reader, () => answer.promise);
    f.value(replacement);
    answer.resolve(true);
    await task;
    assert.equal(f.roleWrites.length, 0);
    assert.deepEqual(f.session.getSnapshot().context, replacement);
  });

for (const [key, value] of [
  ['identity', 'foreign'],
  ['device', 'foreign'],
  ['relay', 'https://foreign.invalid/'],
  ['epoch', 2],
] as const)
  test(`a delayed role review cannot cross ${key}`, async () => {
    const f = await roleFixture(),
      answer = deferred<boolean>();
    const task = f.setRole(reader, () => answer.promise);
    f.scope({ ...scope, [key]: value });
    answer.resolve(true);
    await task;
    assert.equal(f.roleWrites.length, 0);
  });

test('background or same-member close/reopen retires an earlier role confirmation', async () => {
  for (const retire of [true, false]) {
    const f = await roleFixture(),
      answer = deferred<boolean>();
    const task = f.setRole(reader, () => answer.promise);
    if (retire) {
      f.session.retire();
      f.session.activate(scope);
    } else f.session.close();
    await f.open();
    answer.resolve(true);
    await task;
    assert.equal(f.roleWrites.length, 0);
    assert.equal(f.session.getSnapshot().selected, peer);
  }
});

test('role ABA cannot reuse a choice from the earlier membership frontier', async () => {
  const f = await roleFixture(),
    answer = deferred<boolean>();
  const task = f.setRole(reader, () => answer.promise);
  f.value(roleContext({ role: 'admin', role_options: [reader], revision: '2'.repeat(64) }));
  await f.session.refresh(f.session.ticket());
  f.value(
    roleContext({
      revision: '3'.repeat(64),
      role_options: [{ ...reader, revision: 'e'.repeat(64) }],
    }),
  );
  await f.session.refresh(f.session.ticket());
  answer.resolve(true);
  await task;
  assert.equal(f.roleWrites.length, 0);
});

test('role and removal share one confirmation and one dispatched-write fence', async () => {
  const f = await roleFixture(),
    answer = deferred<boolean>();
  let prompts = 0;
  const task = f.setRole(reader, () => {
    ++prompts;
    return answer.promise;
  });
  await f.setRole(admin, async () => {
    ++prompts;
    return true;
  });
  await f.remove(async () => {
    ++prompts;
    return true;
  });
  assert.equal(prompts, 1);
  const ack = deferred<ReturnType<typeof roleContext>>();
  f.roleWrite(() => ack.promise);
  answer.resolve(true);
  await settle();
  await f.session.refresh(f.session.ticket());
  await f.remove();
  await f.setRole(admin);
  assert.equal(f.roleWrites.length, 1);
  assert.equal(f.writes.length, 0);
  ack.resolve(roleContext());
  await task;
});

test('unknown ACK reconciles a retained pending role without resending', async () => {
  const f = await roleFixture();
  f.roleWrite(async (review, option) => {
    f.value({
      ...review,
      state: 'Updating',
      can_remove: false,
      pending_role: option.role,
      role_options: [],
    });
    throw Error('response lost after enqueue');
  });
  await f.setRole();
  await f.setRole();
  assert.equal(f.roleWrites.length, 1);
  assert.equal(f.session.getSnapshot().actionFailed, false);
  assert.equal(f.session.getSnapshot().context?.role, 'member');
});

test('cold canonical ACK after a lost write response shows the actual role without a false failure', async () => {
  const f = await roleFixture();
  f.roleWrite(async () => {
    f.value(roleContext({ role: 'reader', role_options: [admin], revision: '2'.repeat(64) }));
    throw Error('ACK response lost');
  });
  await f.setRole();
  assert.equal(f.roleWrites.length, 1);
  assert.equal(f.session.getSnapshot().context?.role, 'reader');
  assert.equal(f.session.getSnapshot().actionFailed, false);
});

test('refused role stays explicit and the original canonical role remains visible after reread', async () => {
  const f = await roleFixture();
  f.roleWrite(async () => {
    f.value(roleContext({ role_change_failed: true }));
    throw Error('refused');
  });
  await f.setRole();
  assert.equal(f.session.getSnapshot().context?.role, 'member');
  assert.equal(f.session.getSnapshot().actionFailed, true);
  await f.session.refresh(f.session.ticket());
  assert.equal(f.session.getSnapshot().actionFailed, true);
  assert.equal(f.roleWrites.length, 1);
});

for (const patch of [
  { role_options: [{ role: 'owner', revision: 'c'.repeat(64) }] },
  { role_options: [reader, reader] },
  { role_options: [{ role: 'reader', revision: 'invalid' }] },
  { pending_role: 'reader' },
  { role_change_failed: 'false' },
])
  test(`malformed role context ${Object.keys(patch)} fails closed before review`, async () => {
    const f = await roleFixture();
    f.read(async () => ({ ...context(), ...patch }) as any);
    await f.session.refresh(f.session.ticket());
    assert.equal(f.session.getSnapshot().phase, 'failed');
    await f.setRole();
    assert.equal(f.roleWrites.length, 0);
  });
