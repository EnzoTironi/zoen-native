import assert from 'node:assert/strict';
import { test } from 'node:test';
import { editorAgentReactFixture } from './editor-agent-react.fixtures.ts';

test('compiled owned scene entry waits for reactive canvas readiness; same screen and sheet send the displayed pinned head', async () => {
  const f = await editorAgentReactFixture({ ownedScene: true, ready: false });
  try {
    await f.render();
    assert.equal(
      f.controls.has('editor-agent-open'),
      false,
      'old SDK/loading is not a qualified canvas',
    );
    await f.act(() => f.sceneReady(true, 2, 'cd'.repeat(32)));
    assert.equal(f.controls.has('editor-agent-open'), true);
    await f.act(() => f.controls.get('editor-agent-open')!.onPress());
    await f.act(() => f.controls.get('editor-agent-prompt')!.onChangeText('Move the title'));
    await f.act(() => f.sceneReady(false));
    assert.equal(f.controls.get('editor-agent-prompt')!.value, 'Move the title');
    assert.equal(f.controls.get('editor-agent-send')!.disabled, true);
    assert.equal(f.sheets(), 1);
    await f.act(() => f.sceneReady(true, 2, 'cd'.repeat(32)));
    assert.equal(f.controls.get('editor-agent-prompt')!.value, 'Move the title');
    await f.act(() => f.controls.get('editor-agent-send')!.onPress());
    const sent = f.calls.filter((c) => c.op === 'request_app_edit');
    assert.equal(sent.length, 1);
    assert.equal(sent[0]!.expected_version, 2);
    assert.equal(sent[0]!.expected_sha256, 'cd'.repeat(32));
    assert.equal(f.calls.filter((c) => c.op === 'app_document_read_pinned').length, 1);
    assert.equal(
      f.calls.some((c) => c.op === 'app_binary_read'),
      false,
    );
    await f.act(() => f.sceneReady(false));
    assert.equal(f.controls.has('editor-agent-open'), false);
    assert.equal(f.counters().mounts, 1);
    assert.equal(f.counters().unmounts, 0);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

for (const change of ['device', 'identity', undefined] as const)
  test(`compiled scene port refuses stale owner/epoch after same-props reopen (${change ?? 'epoch'})`, async () => {
    const f = await editorAgentReactFixture({ ownedScene: true });
    try {
      await f.render();
      await f.act(() => f.controls.get('editor-agent-open')!.onPress());
      await f.act(() => f.controls.get('editor-agent-prompt')!.onChangeText('Keep this draft'));
      const stale = f.controls.get('editor-agent-send')!.onPress;
      await f.act(() => f.reopen(change));
      await f.act(stale);
      assert.equal(
        f.calls.some((c) => c.op === 'request_app_edit'),
        false,
      );
      assert.deepEqual(f.errors, []);
    } finally {
      await f.close();
    }
  });
