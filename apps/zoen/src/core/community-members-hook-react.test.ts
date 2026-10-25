import assert from 'node:assert/strict';
import { test } from 'node:test';
import { memberHookReactFixture } from './community-members-hook-react.fixtures.ts';
import { deferred, peer } from './community-members.fixtures.ts';
import type { SpaceMemberContext } from './generated/SpaceMemberContext';

for (const change of [undefined, 'identity', 'device', 'relay'] as const)
  test(`actual compiled member hook reactivates after store close/reopen (${change ?? 'same IDs/new epoch'})`, async () => {
    const f = await memberHookReactFixture();
    try {
      await f.render();
      await f.open();
      assert.equal(f.current().state.phase, 'ready');
      const old = f.current().session.ticket();
      assert.ok(old);
      const epoch = f.store.sessionEpoch;
      await f.reopen(change);
      assert.ok(f.store.sessionEpoch > epoch);
      assert.equal(f.current().available, true);
      assert.equal(
        f.current().state.scope,
        f.expectedScope(),
        'new render scope must follow actual store retirement',
      );
      assert.notEqual(f.current().state.scope, old.scope);
      assert.equal(f.current().state.selected, null);
      let prompts = 0;
      await f.act(() =>
        f.current().session.remove(async () => {
          prompts++;
          return true;
        }, old),
      );
      assert.equal(prompts, 0);
      await f.open();
      assert.equal(
        f.current().state.phase,
        'ready',
        'current participant can be opened after cold reopen',
      );
      assert.equal(f.current().state.context?.device_id, f.snapshot.account!.device_id);
      assert.equal(f.current().state.selected, peer);
      assert.equal(f.calls.filter((c) => c.op === 'remove_member_reviewed').length, 0);
      assert.deepEqual(f.errors, []);
    } finally {
      await f.close();
    }
  });

test('actual compiled member hook ignores an old pending read after real retirement/reopen', async () => {
  const f = await memberHookReactFixture();
  const pending = deferred<SpaceMemberContext>();
  try {
    await f.render();
    const old = f.value();
    f.read(pending.promise);
    await f.open();
    assert.equal(f.current().state.phase, 'loading');
    await f.reopen();
    await f.open();
    assert.equal(f.current().state.phase, 'ready');
    const current = f.current().state;
    await f.act(() => pending.resolve({ ...old, space_title: 'Obsolete projection' }));
    assert.equal(f.current().state, current);
    assert.equal(f.current().state.scope, f.expectedScope());
    assert.deepEqual(f.errors, []);
  } finally {
    pending.resolve(f.value());
    await f.close();
  }
});

test('actual Expo focus and foreground hooks retire selected intent and require another explicit open', async () => {
  const f = await memberHookReactFixture();
  try {
    await f.render();
    await f.open();
    const old = f.current().session.ticket();
    assert.ok(old);
    await f.act(() => f.foreground('background'));
    assert.equal(f.current().state.selected, null);
    await f.act(() => f.foreground('active'));
    assert.equal(f.current().state.selected, null);
    await f.open();
    await f.act(() => f.focus(false));
    await f.act(() => f.focus(true));
    assert.equal(f.current().state.selected, null);
    await f.act(() => f.current().session.close(old));
    await f.open();
    assert.equal(f.current().state.phase, 'ready');
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('actual member hook owns addition review across background, blur and account epoch retirement', async () => {
  const f = await memberHookReactFixture();
  try {
    await f.render();
    assert.equal(f.current().additionState.canManage, true);
    await f.act(() =>
      f.current().addition.open(f.current().addition.ticket(f.current().additionState)),
    );
    assert.equal(f.current().additionState.results.length, 1);
    const old = f.current().addition.ticket();
    assert.ok(old);
    const answer = deferred<boolean>();
    let action: Promise<void> | undefined;
    await f.act(() => {
      action = f.current().addition.select(peer, () => answer.promise, old);
    });
    await f.act(() => f.foreground('background'));
    await f.act(() => f.foreground('active'));
    await f.reopen();
    await f.act(async () => {
      answer.resolve(true);
      await action;
    });
    assert.equal(f.calls.filter((c) => c.op === 'add_member_reviewed').length, 0);
    assert.equal(f.current().additionState.open, false);
    assert.notEqual(f.current().additionState.scope, old.scope);
    assert.equal(f.current().additionState.canManage, true);
    await f.act(() => f.current().addition.open());
    assert.equal(f.current().additionState.open, true);
    await f.act(() => f.focus(false));
    await f.act(() => f.focus(true));
    assert.equal(f.current().additionState.open, false);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
