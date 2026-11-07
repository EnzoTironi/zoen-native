import assert from 'node:assert/strict';
import { test } from 'node:test';
import { managedAgentHookFixture, managedView } from './managed-agent-hook.fixtures.ts';

for (const destination of ['chat', 'budget', 'done'] as const) {
  test(`optimized creation screen creates, reviews and opens ${destination} using its visible buttons`, async () => {
    const f = await managedAgentHookFixture(null, 'active', true);
    try {
      await f.act(() => f.screen.type(' My assistant '));
      await f.act(() => f.screen.press('managed-agent-create'));
      assert.equal(f.current().state.view?.state, 'queued');
      const started = f.commands.filter((command) => command.op === 'managed_agent_start');
      assert.equal(started.length, 1);
      assert.equal(started[0]?.name, 'My assistant');
      f.status(async () => managedView('review'));
      await f.tick();
      const consent = f.screen.buttons.get('managed-agent-confirm');
      assert.ok(consent);
      assert.equal(
        f.commands.some((command) => command.op === 'managed_agent_approve'),
        false,
      );
      await f.act(() => f.screen.press('managed-agent-confirm'));
      assert.equal(f.current().state.view?.state, 'installing');
      const approvals = f.commands.filter((command) => command.op === 'managed_agent_approve');
      assert.equal(approvals.length, 1);
      assert.equal(approvals[0]?.operation_id, 'original');
      assert.equal(approvals[0]?.observed_offer_digest, 'reviewed-digest');
      f.status(async () => managedView('registered'));
      await f.tick();
      await f.act(consent.onPress);
      assert.equal(
        f.commands.filter((command) => command.op === 'managed_agent_approve').length,
        1,
      );
      await f.act(() =>
        f.screen.press(
          destination === 'done' ? 'managedAgent.done' : `managed-agent-${destination}`,
        ),
      );
      assert.equal(f.current().state.error, null);
      const completed = f.commands.filter((command) => command.op === 'managed_agent_complete');
      assert.equal(completed.length, destination === 'budget' ? 0 : 1);
      if (destination === 'chat') {
        assert.equal(f.commands.filter((command) => command.op === 'managed_agent_chat').length, 1);
        assert.deepEqual(f.refreshed, [['conversation']]);
        assert.deepEqual(f.navigation, [
          'dismissAll',
          { pathname: '/chat/[id]', params: { id: 'conversation' } },
        ]);
      } else if (destination === 'budget') {
        assert.deepEqual(f.navigation, [
          { pathname: '/account-budget', params: { agent: 'agent' } },
        ]);
      } else {
        assert.deepEqual(f.navigation, ['back']);
        assert.equal(
          f.commands.some((command) => command.op === 'find_people'),
          false,
        );
      }
      assert.equal(
        f.commands.some(
          (command) => command.op.includes('submit') || command.op.includes('policy'),
        ),
        false,
        'Opening the budget never funds the agent automatically',
      );
    } finally {
      await f.close();
    }
  });
}

test('a retired visible create callback cannot create for a reopened session', async () => {
  const f = await managedAgentHookFixture(null, 'active', true);
  try {
    await f.act(() => f.screen.type('Original'));
    const retired = f.screen.buttons.get('managed-agent-create');
    assert.ok(retired);
    await f.reopen();
    await f.act(retired.onPress);
    assert.equal(
      f.commands.some((command) => command.op === 'managed_agent_start'),
      false,
    );
    assert.equal(f.screen.buttons.get('managed-agent-create')?.disabled, true);
    await f.act(() => f.screen.type('Current'));
    await f.act(() => f.screen.press('managed-agent-create'));
    assert.equal(f.commands.filter((command) => command.op === 'managed_agent_start').length, 1);
  } finally {
    await f.close();
  }
});
