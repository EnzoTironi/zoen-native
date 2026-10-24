import assert from 'node:assert/strict';
import { test } from 'node:test';
import { editorAgentReactFixture } from './editor-agent-react.fixtures.ts';

test('compiled native sheet opens/closes while the same document/controller stays mounted', async () => {
  const f = await editorAgentReactFixture();
  try {
    await f.render();
    assert.deepEqual(f.counters(), { mounts: 1, unmounts: 0, flushes: 0, presented: false });
    await f.act(() => f.controls.get('editor-agent-open')!.onPress());
    assert.equal(f.counters().presented, true);
    await f.act(() => f.controls.get('editor-agent-prompt')!.onChangeText('Keep my unsent draft'));
    await f.act(() => f.controls.get('sheet')!.onDismiss());
    await f.act(() => f.controls.get('editor-agent-open')!.onPress());
    assert.equal(f.controls.get('editor-agent-prompt')!.value, 'Keep my unsent draft');
    assert.deepEqual(f.counters(), { mounts: 1, unmounts: 0, flushes: 0, presented: true });
    assert.equal(
      f.calls.some((call) => call.op === 'request_app_edit'),
      false,
    );
    assert.deepEqual(f.routes, []);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
test('compiled prompt sends current typed identity/item/head after flush without route transition', async () => {
  const f = await editorAgentReactFixture();
  try {
    await f.render();
    await f.act(() => f.controls.get('editor-agent-open')!.onPress());
    await f.act(() => f.controls.get('editor-agent-prompt')!.onChangeText('Bold the title'));
    await f.act(() => f.controls.get('editor-agent-send')!.onPress());
    const sent = f.calls.filter((call) => call.op === 'request_app_edit');
    assert.equal(sent.length, 1);
    assert.deepEqual(sent[0], {
      op: 'request_app_edit',
      expected_identity_id: f.snapshot.account!.identity_id,
      expected_device_id: f.snapshot.account!.device_id,
      space_id: f.snapshot.conversations[0]!.id,
      item_id: 'word-item',
      expected_version: 1,
      expected_sha256: 'ab'.repeat(32),
      text: 'Bold the title',
      mutation_id: 'request-1',
    });
    assert.deepEqual(f.counters(), { mounts: 1, unmounts: 0, flushes: 1, presented: true });
    assert.equal(f.controls.get('editor-agent-prompt')!.value, '');
    assert.deepEqual(f.routes, []);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
