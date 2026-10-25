import assert from 'node:assert/strict';
import { test } from 'node:test';
import { memberAdditionReactFixture } from './member-addition-react.fixtures.ts';
import { additionContext, peer, scope, settle } from './member-addition.fixtures.ts';
import { deferred } from './community-members.fixtures.ts';

test('real compiled sheet opens explicit native review then sends one scoped inclusion, without navigation', async () => {
  const f = await memberAdditionReactFixture();
  try {
    await f.render();
    assert.equal(f.sheet().isPresented, false);
    await f.open();
    assert.equal(f.sheet().isPresented, true);
    await f.press(`member-add-${peer}`);
    assert.equal(f.confirmations.length, 1);
    assert.equal(f.writes.length, 1);
    assert.equal(f.writes[0]?.candidate?.kind, 'Agent');
    assert.equal(f.session.getSnapshot().selected?.state, 'Updating');
    assert.deepEqual(f.routes, []);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.act(() => f.session.retire());
    await f.close();
  }
});

test('captured add callback cannot act after dismissal/reopen or same-identity new device', async () => {
  const f = await memberAdditionReactFixture();
  try {
    await f.render();
    await f.open();
    const stale = f.control(`member-add-${peer}`);
    await f.press('common.close');
    await f.open();
    await f.act(async () => {
      stale.onPress();
      await settle();
    });
    assert.equal(f.confirmations.length, 0);
    assert.equal(f.writes.length, 0);
    const current = f.control(`member-add-${peer}`);
    await f.act(() => {
      f.scope({ ...scope, device: 'replacement' });
      f.session.retire();
    });
    await f.act(async () => {
      current.onPress();
      await settle();
    });
    assert.equal(f.writes.length, 0);
  } finally {
    await f.act(() => f.session.retire());
    await f.close();
  }
});

test('real pending confirmation loses role and current-member duplicate remains disabled', async () => {
  const f = await memberAdditionReactFixture();
  try {
    await f.render();
    await f.open();
    const answer = deferred<boolean>();
    f.confirm(() => answer.promise);
    await f.press(`member-add-${peer}`);
    await f.act(async () => {
      f.value({ ...additionContext(), can_add: false, can_manage_members: false });
      answer.resolve(true);
      await settle();
    });
    assert.equal(f.writes.length, 0);
    await f.act(async () => {
      f.value({
        ...additionContext(),
        can_add: false,
        state: 'Active',
        member: additionContext().candidate,
        role: 'member',
      });
      await f.session.refresh();
    });
    assert.equal(f.control(`member-add-${peer}`).disabled, true);
    assert.equal(f.writes.length, 0);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.act(() => f.session.retire());
    await f.close();
  }
});

test('real failed join keeps the typed search and selected profile for explicit retry', async () => {
  const f = await memberAdditionReactFixture();
  try {
    await f.render();
    await f.open();
    await f.search('participant');
    f.write(async () => {
      throw Error('refused by relay');
    });
    await f.press(`member-add-${peer}`);
    assert.equal(f.session.getSnapshot().query, 'participant');
    assert.equal(f.session.getSnapshot().failed, true);
    assert.equal(f.session.getSnapshot().selected?.member_id, peer);
    assert.equal(f.writes.length, 1);
    assert.equal(f.sheet().isPresented, true);
  } finally {
    await f.act(() => f.session.retire());
    await f.close();
  }
});
