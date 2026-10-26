import assert from 'node:assert/strict';
import { test } from 'node:test';
import { memberReactFixture } from './community-members-react.fixtures.ts';
import { roleContext, reader, admin, peer, scope, deferred } from './member-role.fixtures.ts';
async function opened() {
  const f = memberReactFixture();
  f.value(roleContext());
  await f.render();
  assert.equal(f.sheet().isPresented, false);
  await f.open();
  return f;
}
test('actual Compiler sheet uses an operation-bound role and never changes the canonical role optimistically', async () => {
  const f = await opened();
  try {
    assert.deepEqual(
      f.menu().actions.map((action) => action.id),
      [reader.revision, admin.revision],
    );
    await f.act(() => f.menu().onPressAction({ nativeEvent: { event: reader.revision } }));
    assert.equal(f.confirmations.length, 1);
    assert.equal(f.roleWrites.length, 1);
    assert.deepEqual(f.roleWrites[0]?.option, reader);
    assert.equal(f.session.getSnapshot().context?.role, 'member');
    assert.equal(f.session.getSnapshot().context?.pending_role, 'reader');
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
test('two queued native role callbacks and Remove share one actual compiled review', async () => {
  const f = await opened(),
    answer = deferred<boolean>();
  try {
    f.confirm(() => answer.promise);
    const menu = f.menu(),
      remove = f.control('member-remove');
    await f.act(() => {
      menu.onPressAction({ nativeEvent: { event: reader.revision } });
      menu.onPressAction({ nativeEvent: { event: admin.revision } });
      remove.onPress();
    });
    assert.equal(f.confirmations.length, 1);
    assert.equal(f.roleWrites.length, 0);
    await f.act(() => answer.resolve(true));
    assert.equal(f.roleWrites.length, 1);
    assert.equal(f.writes.length, 0);
    assert.deepEqual(f.errors, []);
  } finally {
    answer.resolve(false);
    await f.close();
  }
});
for (const change of ['reopen', 'option', 'device'] as const)
  test(`compiled stale native menu cannot cross ${change}`, async () => {
    const f = await opened();
    try {
      const menu = f.menu();
      if (change === 'reopen') {
        await f.press('common.close');
        await f.open();
      } else if (change === 'option') {
        f.value(roleContext({ role_options: [{ ...reader, revision: 'e'.repeat(64) }] }));
        await f.act(() => f.session.refresh(f.session.ticket()));
      } else
        await f.act(() => {
          f.scope({ ...scope, device: 'e'.repeat(64), epoch: 2 });
          f.session.retire();
        });
      await f.act(() => menu.onPressAction({ nativeEvent: { event: reader.revision } }));
      assert.equal(f.confirmations.length, 0);
      assert.equal(f.roleWrites.length, 0);
      assert.equal(f.writes.length, 0);
      assert.deepEqual(f.errors, []);
    } finally {
      await f.close();
    }
  });
test('compiled role confirmation expires across background and reactivation', async () => {
  const f = await opened(),
    answer = deferred<boolean>();
  try {
    f.confirm(() => answer.promise);
    await f.act(() => f.menu().onPressAction({ nativeEvent: { event: reader.revision } }));
    await f.act(() => {
      f.session.retire();
      f.session.activate(scope);
    });
    await f.open();
    await f.act(() => answer.resolve(true));
    assert.equal(f.session.getSnapshot().selected, peer);
    assert.equal(f.roleWrites.length, 0);
    assert.equal(f.session.getSnapshot().busy, false);
    assert.deepEqual(f.errors, []);
  } finally {
    answer.resolve(false);
    await f.close();
  }
});

test('actual Expo focus hook and store epoch retire a pending role review', async () => {
  const { memberHookReactFixture } = await import('./community-members-hook-react.fixtures.ts');
  for (const boundary of ['foreground', 'reopen'] as const) {
    const f = await memberHookReactFixture(),
      answer = deferred<boolean>();
    let pending: Promise<void> | undefined;
    try {
      f.memberState({ role_options: [reader, admin] });
      await f.render();
      await f.open();
      const current = f.current();
      await f.act(() => {
        pending = current.session.setRole(reader, () => answer.promise, current.session.ticket());
      });
      if (boundary === 'foreground') {
        await f.act(() => f.foreground('background'));
        await f.act(() => f.foreground('active'));
      } else await f.reopen('device');
      await f.act(async () => {
        answer.resolve(true);
        await pending;
      });
      assert.equal(f.calls.filter((call) => call.op === 'set_member_role_reviewed').length, 0);
      assert.equal(f.current().state.selected, null);
      assert.deepEqual(f.errors, []);
    } finally {
      answer.resolve(false);
      await f.close();
    }
  }
});
