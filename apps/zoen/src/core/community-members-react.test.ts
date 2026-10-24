import assert from 'node:assert/strict';
import { test } from 'node:test';
import { memberReactFixture } from './community-members-react.fixtures.ts';
import { deferred, fixture, peer, scope } from './community-members.fixtures.ts';

for (const action of ['common.close', 'member-open-profile', 'member-remove'])
  test(`real compiled member sheet ${action} uses the current ticket after its closed first render`, async () => {
    const f = memberReactFixture();
    try {
      await f.render();
      assert.equal(f.sheet().isPresented, false);
      await f.open();
      assert.equal(f.sheet().isPresented, true);
      assert.equal(f.session.getSnapshot().phase, 'ready');
      assert.equal(f.control(action).disabled ?? false, false);
      await f.press(action);
      if (action === 'member-remove') {
        assert.equal(f.confirmations.length, 1, 'the current visible Remove opens real review');
        assert.equal(f.writes.length, 1);
        assert.equal(f.writes[0]?.review.member_id, peer);
        assert.equal(f.session.getSnapshot().context?.state, 'Removing');
      } else {
        assert.equal(
          f.session.getSnapshot().selected,
          null,
          'current close/profile retires the sheet',
        );
        assert.equal(f.writes.length, 0);
        assert.deepEqual(
          f.routes,
          action === 'member-open-profile'
            ? [{ pathname: '/person/[id]', params: { id: peer } }]
            : [],
        );
      }
      assert.deepEqual(f.errors, []);
    } finally {
      await f.close();
    }
  });

test('compiled stale controls stay retired through same-member close/reopen and account-device replacement', async () => {
  const f = memberReactFixture();
  try {
    await f.render();
    await f.open();
    const old = ['common.close', 'member-open-profile', 'member-remove'].map((id) => f.control(id));
    await f.press('common.close');
    assert.equal(
      f.session.getSnapshot().selected,
      null,
      'retire a genuinely actionable current sheet first',
    );
    await f.open();
    await f.act(() => {
      for (const control of old) control.onPress();
    });
    assert.equal(f.session.getSnapshot().selected, peer);
    assert.equal(f.confirmations.length, 0);
    assert.equal(f.routes.length, 0);
    assert.equal(f.writes.length, 0);
    const current = f.control('member-remove');
    await f.act(() => {
      f.scope({ ...scope, device: 'e'.repeat(64), epoch: 2 });
      f.session.retire();
      current.onPress();
    });
    assert.equal(f.confirmations.length, 0);
    assert.equal(f.writes.length, 0);
    assert.equal(f.sheet().isPresented, false);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('compiled pending confirmation cannot remove after retirement and explicit reactivation', async () => {
  const f = memberReactFixture();
  const answer = deferred<boolean>();
  try {
    await f.render();
    await f.open();
    f.confirm(() => answer.promise);
    const remove = f.control('member-remove');
    await f.act(() => {
      remove.onPress();
      remove.onPress();
    });
    assert.equal(f.confirmations.length, 1, 'two queued presses cannot duplicate review');
    await f.act(() => {
      f.session.retire();
      f.session.activate(scope);
    });
    await f.open();
    await f.act(() => answer.resolve(true));
    assert.equal(f.session.getSnapshot().selected, peer);
    assert.equal(f.session.getSnapshot().busy, false);
    assert.equal(f.writes.length, 0);
    assert.deepEqual(f.errors, []);
  } finally {
    answer.resolve(false);
    await f.close();
  }
});

test('render tickets require the exact current observed snapshot, including same-member reopen', async () => {
  const f = fixture();
  const closed = f.session.getSnapshot();
  await f.open();
  const current = f.session.getSnapshot();
  assert.equal(f.session.ticket(closed), null);
  assert.equal(
    f.session.ticket({ ...current }),
    null,
    'a copied projection cannot mint current intent',
  );
  assert.ok(f.session.ticket(current));
  await f.open();
  assert.equal(f.session.ticket(current), null);
  assert.ok(f.session.ticket(f.session.getSnapshot()));
  f.session.retire();
  assert.equal(f.session.ticket(current), null);
});
