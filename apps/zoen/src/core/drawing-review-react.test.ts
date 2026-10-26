import assert from 'node:assert/strict';
import { test } from 'node:test';
import { editorAgentReactFixture } from './editor-agent-react.fixtures.ts';
import { t } from '../i18n/index.ts';
import type { AgentRequestDto } from './generated/AgentRequestDto';

async function ready() {
  const f = await editorAgentReactFixture({ ownedScene: true, visualReview: true });
  const request = {
    id: f.review.request_id,
    space_id: f.review.space_id,
    space_title: 'Chat',
    agent: { id: f.agentId, name: 'My Agent', is_mine: true, kind: 'Agent', tint_hex: '#f00' },
    title: 'Change drawing',
    detail: JSON.stringify({ sceneJson: f.review.after_scene_json }),
    audience: 'Chat',
    action_label: 'Edit',
    cost_cents: null,
    opened_ms: String(Date.now()),
    status: 'Pending',
    reason: 'Review',
    tool_approval: {
      artifact_id: 'word-item',
      artifact_title: 'Drawing',
      version: 1,
      operation: 'Replace scene',
      content_hash: f.review.content_hash,
      expires_ms: String(Date.now() + 60000),
      preview_kind: 'scene',
      canonical_tool: 'replace_scene',
    },
  } as AgentRequestDto;
  await f.render();
  await f.act(() => f.remote([], [request]));
  await f.act(() => f.controls.get('editor-agent-open')!.onPress());
  return Object.assign(f, { request });
}
const approve = (f: Awaited<ReturnType<typeof ready>>) =>
  f.controls.get(`label:${t('agents.allowOnce')}`)!;
const decisions = (f: Awaited<ReturnType<typeof ready>>) =>
  f.calls.filter((c) => c.op === 'decide_tool_request');
const open = async (f: Awaited<ReturnType<typeof ready>>) =>
  f.act(() => f.controls.get('drawing-review-open-request')!.onPress());
const load = async (f: Awaited<ReturnType<typeof ready>>, side: string) =>
  f.act(() => f.controls.get(`drawing-review-${side}-request`)!.onLoad());

test('compiled actual RequestCard in the same Agent sheet shows real PNG sources, hides JSON, and waits for BOTH images before exact approval', async () => {
  const f = await ready();
  try {
    assert.equal(approve(f).disabled, true);
    assert.equal(
      [...f.controls.keys()].some((k) => k.includes('sceneJson')),
      false,
    );
    await f.act(() => approve(f).onPress());
    assert.equal(decisions(f).length, 0);
    await open(f);
    const before = f.controls.get('drawing-review-before-request')!,
      after = f.controls.get('drawing-review-after-request')!;
    assert.ok(before?.source.uri.startsWith('data:image/png;base64,'));
    assert.ok(after?.source.uri.startsWith('data:image/png;base64,'));
    assert.notEqual(before.source.uri, after.source.uri);
    assert.equal(before.cachePolicy, 'none');
    assert.equal(after.contentFit, 'contain');
    await load(f, 'before');
    assert.equal(approve(f).disabled, true);
    await load(f, 'after');
    assert.equal(approve(f).disabled, false);
    await f.act(() => approve(f).onPress());
    assert.equal(decisions(f).length, 1);
    assert.equal(decisions(f)[0].observed_content_hash, f.review.content_hash);
    assert.equal(decisions(f)[0].decision, 'ApproveOnce');
    assert.equal(f.counters().mounts, 1);
    assert.equal(f.counters().unmounts, 0);
    assert.deepEqual(f.routes, []);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

for (const change of ['device', undefined] as const)
  test(`late PNG and retained approval handler cannot survive ${change ?? 'epoch'} owner reopening`, async () => {
    const f = await ready();
    try {
      await open(f);
      const before = f.controls.get('drawing-review-before-request')!,
        after = f.controls.get('drawing-review-after-request')!;
      await load(f, 'before');
      await load(f, 'after');
      const stale = approve(f).onPress;
      await f.act(() => f.reopen(change));
      await f.act(() => {
        before.onLoad();
        after.onLoad();
      });
      await f.act(stale);
      assert.equal(decisions(f).length, 0);
      assert.equal(f.controls.has('drawing-review-after-request'), false);
      assert.deepEqual(f.errors, []);
    } finally {
      await f.close();
    }
  });

test('base/grant revocation invalidates loaded images and a stale Approve callback before any publication', async () => {
  const f = await ready();
  try {
    await open(f);
    await load(f, 'before');
    await load(f, 'after');
    const stale = approve(f).onPress,
      loadOld = f.controls.get('drawing-review-after-request')!.onLoad;
    await f.act(() => {
      f.previewAllowed(false);
      f.remote([], []);
    });
    await f.act(loadOld);
    await f.act(stale);
    assert.equal(decisions(f).length, 0);
    assert.equal(f.controls.has('drawing-review-after-request'), false);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('a failed native PNG decode retires both image callbacks and cannot enable approval later', async () => {
  const f = await ready();
  try {
    await open(f);
    const before = f.controls.get('drawing-review-before-request')!,
      after = f.controls.get('drawing-review-after-request')!;
    await f.act(() => before.onError({ error: 'decode' }));
    await f.act(() => {
      before.onLoad();
      after.onLoad();
    });
    assert.equal(approve(f).disabled, true);
    await f.act(() => approve(f).onPress());
    assert.equal(decisions(f).length, 0);
    assert.equal(f.controls.has('drawing-review-after-request'), false);
  } finally {
    await f.close();
  }
});

test('bursty canonical invalidations retire immediately and coalesce exports behind the current frame operation', async () => {
  const f = await ready();
  let release!: () => void;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  try {
    f.holdPreview(gate);
    await open(f);
    for (let i = 0; i < 6; i++) await f.act(() => f.remote([], [f.request]));
    assert.equal(f.calls.filter((c) => c.op === 'scene_tool_review').length, 1);
    assert.equal(approve(f).disabled, true);
    f.holdPreview(null);
    await f.act(release);
    assert.equal(f.calls.filter((c) => c.op === 'scene_tool_review').length, 4);
    await load(f, 'before');
    await load(f, 'after');
    assert.equal(approve(f).disabled, false);
    assert.equal(decisions(f).length, 0);
  } finally {
    release();
    await f.close();
  }
});
