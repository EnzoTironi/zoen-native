import assert from 'node:assert/strict';
import { test } from 'node:test';
import { accountBudgetReactFixture } from './account-budget-react.fixtures.ts';
import { deferred } from './store.fixtures.ts';

test('compiled budget selector discovers a newly registered owned agent without reopening the sheet', async () => {
  const f = await accountBudgetReactFixture(true);
  try {
    assert.equal(f.ui.rows.length, 0);
    await f.addAgent();
    assert.equal(f.ui.rows.length, 1);
    await f.act(() => f.ui.rows[0]!.onPress());
    assert.equal(f.selection()?.agentId, 'owned-agent');
    assert.equal(f.commands.filter((op) => op === 'start_owner_policy_direct').length, 1);
  } finally {
    await f.close();
  }
});

test('compiled budget selector reopens the same account and ignores callbacks from its retired activation', async () => {
  const f = await accountBudgetReactFixture();
  try {
    const old = f.ui.rows[0]!,
      retiredApp = [...f.listeners][0]!;
    await f.reopen();
    await f.act(() => {
      retiredApp('background');
      retiredApp('active');
      old.onPress();
    });
    assert.equal(f.commands.filter((op) => op === 'start_owner_policy_direct').length, 0);
    await f.act(() => f.ui.rows[0]!.onPress());
    assert.equal(f.selection()?.spaceId, 'private-budget');
  } finally {
    await f.close();
  }
});

test('a compiled budget selection returning after background cannot replace the current selection', async () => {
  const f = await accountBudgetReactFixture(),
    pending = deferred<unknown>();
  try {
    f.hold(pending.promise);
    await f.act(() => f.ui.rows[0]!.onPress());
    await f.app('background');
    await f.app('active');
    await f.act(() => pending.resolve('private-budget'));
    assert.equal(f.selection(), null);
    f.hold(Promise.resolve('private-budget'));
    await f.act(() => f.ui.rows[0]!.onPress());
    assert.equal(f.selection()?.spaceId, 'private-budget');
  } finally {
    pending.resolve(null);
    await f.close();
  }
});

test('an explicit Budget destination opens its owned agent directly without a redundant selector tap', async () => {
  const f = await accountBudgetReactFixture(false, 'owned-agent');
  try {
    assert.equal(f.selection()?.agentId, 'owned-agent');
    assert.equal(f.commands.filter((op) => op === 'start_owner_policy_direct').length, 1);
    assert.ok(f.commands.every((op) => op === 'snapshot' || op === 'start_owner_policy_direct'));
  } finally {
    await f.close();
  }
});

test('a callback from a retired budget activation cannot cancel the current pending selection', async () => {
  const f = await accountBudgetReactFixture(),
    pending = deferred<unknown>();
  try {
    const retiredApp = [...f.listeners][0]!;
    await f.reopen();
    f.hold(pending.promise);
    await f.act(() => f.ui.rows[0]!.onPress());
    await f.act(() => {
      retiredApp('background');
      retiredApp('active');
    });
    await f.act(() => pending.resolve('private-budget'));
    assert.equal(f.selection()?.spaceId, 'private-budget');
  } finally {
    pending.resolve(null);
    await f.close();
  }
});

test('a direct Budget destination cannot use a foreign or unknown agent', async () => {
  const f = await accountBudgetReactFixture(false, 'foreign-agent');
  try {
    assert.equal(f.selection(), null);
    assert.equal(f.ui.rows.length, 0);
    assert.equal(f.commands.filter((op) => op === 'start_owner_policy_direct').length, 0);
  } finally {
    await f.close();
  }
});

test('a failed direct Budget open is retried only by an explicit row tap', async () => {
  const pending = deferred<unknown>();
  const f = await accountBudgetReactFixture(false, 'owned-agent', pending.promise);
  try {
    assert.equal(f.commands.filter((op) => op === 'start_owner_policy_direct').length, 1);
    await f.act(() => pending.reject(Error('Connection unavailable')));
    await f.app('background');
    await f.app('active');
    await f.addAgent();
    assert.equal(f.commands.filter((op) => op === 'start_owner_policy_direct').length, 1);
    f.hold(Promise.resolve('private-budget'));
    await f.act(() => f.ui.rows[0]!.onPress());
    assert.equal(f.commands.filter((op) => op === 'start_owner_policy_direct').length, 2);
    assert.equal(f.selection()?.agentId, 'owned-agent');
  } finally {
    pending.resolve(null);
    await f.close();
  }
});
