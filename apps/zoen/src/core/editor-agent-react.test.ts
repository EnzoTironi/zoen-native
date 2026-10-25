import assert from 'node:assert/strict';
import { test } from 'node:test';
import { editorAgentReactFixture } from './editor-agent-react.fixtures.ts';
import type { TimelineEntry } from './types';
import type { AgentRequestDto } from './generated/AgentRequestDto';

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

test('remote canonical events refresh the open sheet with inline source replies and reviews without remounting', async () => {
  const f = await editorAgentReactFixture();
  try {
    await f.render();
    await f.act(() => f.controls.get('editor-agent-open')!.onPress());
    await f.act(() => f.controls.get('editor-agent-prompt')!.onChangeText('Center this paragraph'));
    await f.act(() => f.controls.get('editor-agent-send')!.onPress());
    const reply = {
      id: 'agent-answer',
      author: { kind: 'Agent', name: 'My agent' },
      deleted: false,
      reply_to: { id: 'canonical-request' },
      in_thread: null,
      kind: { Message: { text: 'Review the centered paragraph', card: null } },
    } as TimelineEntry;
    const review = {
      id: 'review-current',
      space_id: f.snapshot.conversations[0]!.id,
      agent: { is_mine: true },
      status: 'Pending',
      tool_approval: {
        artifact_id: 'word-item',
        artifact_title: 'My document',
        version: 1,
        operation: 'Center paragraph',
        expires_ms: String(Date.now() + 60_000),
        content_hash: 'cd'.repeat(32),
      },
    } as AgentRequestDto;
    await f.act(() => f.remote([reply], [review]));
    assert.equal(f.controls.has('markdown:Review the centered paragraph'), true);
    assert.equal(f.controls.get('review:review-current')?.embedded, true);
    assert.deepEqual(f.counters(), { mounts: 1, unmounts: 0, flushes: 1, presented: true });
    assert.equal(f.calls.filter(({ op }) => op === 'request_app_edit').length, 1);
    assert.deepEqual(f.routes, []);
    await f.act(() =>
      f.remote([{ ...reply, reply_to: { ...reply.reply_to!, id: 'another-request' } }], []),
    );
    assert.equal(f.controls.has('markdown:Review the centered paragraph'), false);
    assert.equal(f.controls.has('review:review-current'), false);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
