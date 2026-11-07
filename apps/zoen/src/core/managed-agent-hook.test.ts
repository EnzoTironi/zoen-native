import assert from 'node:assert/strict';
import { test } from 'node:test';
import { managedAgentHookFixture, managedView } from './managed-agent-hook.fixtures.ts';
import { deferred } from './store.fixtures.ts';
import type { ManagedEnrollmentDto } from './generated/ManagedEnrollmentDto';

const effects = new Set([
  'managed_agent_start',
  'managed_agent_approve',
  'managed_agent_cancel',
  'managed_agent_complete',
  'managed_agent_discard',
  'managed_agent_chat',
]);

test('open review observes consent and completion on another device without repeating any effect', async () => {
  const f = await managedAgentHookFixture(managedView());
  try {
    assert.equal(f.current().state.view?.state, 'review');
    assert.ok(
      [...f.timers.values()].every((timer) => timer.delay <= 2000),
      'Do not wait for offer expiry',
    );
    const original = f.current().state.view;
    const ticket = f.current().ticket;
    f.status(async () => managedView('installing'));
    await f.tick();
    assert.equal(f.current().state.view?.state, 'installing');
    await f.act(() => f.current().session.approve(ticket, original));
    f.status(async () => managedView('registered'));
    await f.tick();
    assert.equal(f.current().state.view?.state, 'registered');
    assert.equal(f.timers.size, 0, 'Registered installation is terminal for polling');
    assert.equal(f.commands.filter((command) => effects.has(command.op)).length, 0);
  } finally {
    await f.close();
  }
});

test('empty recovery discovery notices a request from another device and preserves the selectable page', async () => {
  const f = await managedAgentHookFixture();
  try {
    f.page({
      requests: [{ operation_id: 'remote', name: 'Remote assistant', state: 'review' }],
      next_operation: null,
    });
    await f.tick();
    assert.equal(f.current().state.recoveries[0]?.operation_id, 'remote');
    assert.equal(f.timers.size, 0, 'Do not replace the page while a person selects a request');
    assert.equal(f.commands.filter((command) => effects.has(command.op)).length, 0);
  } finally {
    await f.close();
  }
});

test('background, session replacement and retired callbacks cannot borrow the current activation', async () => {
  const f = await managedAgentHookFixture(managedView());
  try {
    const retiredApp = [...f.appListeners][0]!;
    const retiredPreflight = [...f.preflights][0]!;
    const retiredTimer = [...f.timers.values()][0]!.callback;
    await f.reopen();
    const current = f.current().session.ticket(),
      calls = f.commands.length;
    await f.act(() => {
      retiredPreflight();
      retiredApp('background');
      retiredApp('active');
      retiredTimer();
    });
    assert.equal(f.current().session.ticket(), current);
    assert.equal(f.current().state.view?.state, 'review');
    assert.equal(f.commands.length, calls);
    const foregroundTimer = [...f.timers.values()][0]!.callback;
    await f.app('background');
    assert.equal(f.timers.size, 0);
    await f.act(foregroundTimer);
    assert.equal(f.commands.length, calls);
    await f.app('active');
    assert.equal(f.current().state.view?.state, 'review');
    assert.ok(f.commands.length > calls);
  } finally {
    await f.close();
  }
});

test('status observation is single flight, discards late old data and stops on transport failure', async () => {
  const f = await managedAgentHookFixture(managedView());
  const pending = deferred<ManagedEnrollmentDto | null>();
  try {
    f.status(() => pending.promise);
    await f.tick();
    const calls = f.commands.length;
    assert.equal(f.timers.size, 0, 'Never schedule a second read while the first is unresolved');
    await f.act(() => f.current().session.refresh());
    assert.equal(f.commands.length, calls);
    await f.app('background');
    await f.act(() => pending.resolve(managedView('registered')));
    assert.equal(f.current().state.view, null);
    f.status(async () => {
      throw Error('Transport unavailable');
    });
    await f.app('active');
    assert.equal(f.current().state.error, 'status');
    assert.equal(f.timers.size, 0, 'A failure requires explicit retry');
    assert.equal(f.commands.filter((command) => effects.has(command.op)).length, 0);
  } finally {
    pending.resolve(null);
    await f.close();
  }
});

test('mounting a focused sheet while the app is backgrounded cannot start discovery or polling', async () => {
  const f = await managedAgentHookFixture(null, 'background');
  try {
    assert.equal(f.current().active, false);
    assert.equal(f.commands.length, 0);
    assert.equal(f.timers.size, 0);
    await f.app('active');
    assert.equal(f.current().active, true);
    assert.equal(f.current().state.ready, true);
    assert.ok(f.commands.some((command) => command.op === 'managed_agent_discover'));
    assert.equal(f.timers.size, 1);
  } finally {
    await f.close();
  }
});
