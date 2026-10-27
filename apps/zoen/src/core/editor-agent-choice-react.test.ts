import assert from 'node:assert/strict';
import { test } from 'node:test';
import { editorAgentReactFixture } from './editor-agent-react.fixtures.ts';

const tick = () => new Promise<void>((done) => setImmediate(done));
for (const ownedScene of [true, false])
  test(`compiled native chooser requires an exact Agent, old Send cannot retarget (${ownedScene ? 'owned scene' : 'legacy binary'})`, async () => {
    const f = await editorAgentReactFixture({ ownedScene });
    try {
      f.targets.multiple(true);
      await f.render();
      await f.act(() => f.controls.get('editor-agent-open')!.onPress());
      await f.act(() => f.controls.get('editor-agent-prompt')!.onChangeText('Move the title'));
      assert.equal(f.controls.get('editor-agent-choice')!.selectedValue, '');
      assert.equal(f.controls.get('editor-agent-send')!.disabled, true);
      const emptySend = f.controls.get('editor-agent-send')!.onPress;
      await f.act(() => f.controls.get('editor-agent-choice')!.onValueChange(f.agentId));
      await f.act(emptySend);
      assert.equal(
        f.calls.some((c) => c.op === 'request_app_edit'),
        false,
      );
      const oldSend = f.controls.get('editor-agent-send')!.onPress;
      await f.act(() => f.controls.get('editor-agent-choice')!.onValueChange(f.targets.second));
      await f.act(oldSend);
      assert.equal(
        f.calls.some((c) => c.op === 'request_app_edit'),
        false,
      );
      await f.act(() => f.controls.get('editor-agent-send')!.onPress());
      const request = f.calls.find((c) => c.op === 'request_app_edit');
      assert.equal(request.target_agent_id, f.targets.second);
      assert.equal(request.expected_version, 1);
      assert.equal(request.expected_sha256, 'ab'.repeat(32));
      assert.equal(typeof request.expected_target_revision, 'string');
      assert.equal(Boolean(request.expected_resource_uri), ownedScene);
      assert.equal(f.counters().mounts, 1);
      assert.deepEqual(f.routes, []);
      assert.deepEqual(f.errors, []);
    } finally {
      await f.close();
    }
  });

test('permission-required Agent stays in the same sheet, explicit consent names the conversation and refreshes Core eligibility', async () => {
  const f = await editorAgentReactFixture({ ownedScene: true });
  try {
    f.targets.permission(true);
    await f.render();
    await f.act(() => f.controls.get('editor-agent-open')!.onPress());
    await f.act(() => f.controls.get('editor-agent-prompt')!.onChangeText('Keep my prompt'));
    assert.equal(
      f.controls.has('editor-agent-add'),
      false,
      'already included Agent must not be offered as missing',
    );
    assert.equal(f.controls.get('editor-agent-send')!.disabled, true);
    await f.act(() => f.controls.get(`editor-agent-permissions-${f.agentId}`)!.onPress());
    assert.equal(
      f.controls.get('editor-agent-allow-suggest')!.label,
      'Allow suggestions in this chat',
    );
    await f.act(() => f.controls.get('editor-agent-allow-suggest')!.onPress());
    await f.act(tick);
    assert.equal(f.targets.confirmations.length, 1);
    assert.match(f.targets.confirmations[0]!.message, /My agent/);
    assert.match(f.targets.confirmations[0]!.message, /Conversation/);
    const grants = f.calls.filter((c) => c.op === 'set_trust');
    assert.equal(grants.length, 1);
    assert.deepEqual(grants[0], {
      op: 'set_trust',
      agent_id: f.agentId,
      space_id: f.snapshot.conversations[0]!.id,
      level: 'Suggest',
    });
    assert.equal(f.controls.get('editor-agent-prompt')!.value, 'Keep my prompt');
    assert.equal(f.controls.get('editor-agent-choice')!.selectedValue, f.agentId);
    assert.equal(f.controls.get('editor-agent-send')!.disabled, false);
    assert.equal(f.counters().mounts, 1);
    assert.equal(f.sheets(), 1);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

for (const change of ['grant', 'membership', 'head'] as const)
  test(`compiled stale choice/send is fenced after ${change}, while the prompt survives`, async () => {
    const f = await editorAgentReactFixture({ ownedScene: true });
    try {
      f.targets.multiple(true);
      await f.render();
      await f.act(() => f.controls.get('editor-agent-open')!.onPress());
      await f.act(() => f.controls.get('editor-agent-choice')!.onValueChange(f.agentId));
      await f.act(() => f.controls.get('editor-agent-prompt')!.onChangeText('Human draft'));
      const stale = f.controls.get('editor-agent-send')!.onPress;
      const stalePicker = f.controls.get('editor-agent-choice')!.onValueChange;
      await f.act(() => {
        if (change === 'membership') f.members('waiting');
        else if (change === 'grant') {
          f.targets.permission(true);
          f.targetsChanged();
        } else f.sceneReady(false, 2, 'cd'.repeat(32));
      });
      await f.act(() => stalePicker(f.targets.second));
      await f.act(stale);
      assert.equal(
        f.calls.some((c) => c.op === 'request_app_edit'),
        false,
      );
      assert.equal(f.controls.get('editor-agent-prompt')!.value, 'Human draft');
      assert.equal(f.controls.get('editor-agent-send')!.disabled, true);
      assert.deepEqual(f.errors, []);
    } finally {
      await f.close();
    }
  });

for (const change of ['device', 'membership', 'dismiss'] as const)
  test(`stale ${change} during explicit permission confirmation cannot grant through its callback`, async () => {
    const f = await editorAgentReactFixture({ ownedScene: true });
    let resolve!: (answer: boolean) => void;
    f.targets.permission(true);
    f.targets.holdConfirm(
      new Promise<boolean>((done) => {
        resolve = done;
      }),
    );
    try {
      await f.render();
      await f.act(() => f.controls.get('editor-agent-open')!.onPress());
      await f.act(() => f.controls.get(`editor-agent-permissions-${f.agentId}`)!.onPress());
      await f.act(() => f.controls.get('editor-agent-allow-suggest')!.onPress());
      await f.act(() =>
        change === 'device'
          ? f.reopen('device')
          : change === 'membership'
            ? f.members('waiting')
            : f.controls.get('sheet')!.onDismiss(),
      );
      await f.act(() => resolve(true));
      assert.equal(
        f.calls.some((c) => c.op === 'set_trust'),
        false,
      );
      assert.deepEqual(f.errors, []);
    } finally {
      resolve(false);
      await f.close();
    }
  });

test('compiled picker cannot retarget a held request after grant changes; retry uses the exact typed command', async () => {
  const f = await editorAgentReactFixture({ ownedScene: true });
  try {
    f.targets.multiple(true);
    await f.render();
    await f.act(() => f.controls.get('editor-agent-open')!.onPress());
    await f.act(() => f.controls.get('editor-agent-choice')!.onValueChange(f.agentId));
    const stalePicker = f.controls.get('editor-agent-choice')!.onValueChange;
    await f.act(() => f.controls.get('editor-agent-prompt')!.onChangeText('One human intent'));
    f.targets.loseAck();
    await f.act(() => f.controls.get('editor-agent-send')!.onPress());
    assert.equal(f.controls.get('editor-agent-choice')!.enabled, false);
    assert.equal(f.controls.get('editor-agent-prompt')!.editable, false);
    await f.act(() => {
      f.targets.permission(true);
      f.targetsChanged();
    });
    await f.act(() => stalePicker(f.targets.second));
    await f.act(() => f.controls.get('editor-agent-choice')!.onValueChange(f.targets.second));
    assert.equal(f.controls.get('editor-agent-choice')!.selectedValue, f.agentId);
    await f.act(() => f.controls.get('editor-agent-send')!.onPress());
    const commands = f.calls.filter((c) => c.op === 'request_app_edit');
    assert.equal(commands.length, 2);
    assert.deepEqual(commands[0], commands[1]);
    assert.equal(f.controls.get('editor-agent-prompt')!.value, '');
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('an ordinary canonical message retains a choice only after Core confirms the same target revision', async () => {
  const f = await editorAgentReactFixture({ ownedScene: true });
  try {
    f.targets.multiple(true);
    await f.render();
    await f.act(() => f.controls.get('editor-agent-open')!.onPress());
    await f.act(() => f.controls.get('editor-agent-choice')!.onValueChange(f.targets.second));
    await f.act(() => f.controls.get('editor-agent-prompt')!.onChangeText('Keep chosen Agent'));
    const staleSend = f.controls.get('editor-agent-send')!.onPress;
    await f.act(() => f.remote([]));
    assert.equal(f.controls.get('editor-agent-choice')!.selectedValue, f.targets.second);
    await f.act(staleSend);
    assert.equal(
      f.calls.some((c) => c.op === 'request_app_edit'),
      false,
    );
    await f.act(() => f.controls.get('editor-agent-send')!.onPress());
    assert.equal(
      f.calls.find((c) => c.op === 'request_app_edit').target_agent_id,
      f.targets.second,
    );
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
