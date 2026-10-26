import assert from 'node:assert/strict';
import { test } from 'node:test';
import { appRequestReviewReactFixture } from './app-request-review-react.fixtures.ts';
import { accountScope, accountScopeKey } from './account-scope.ts';
import { deferred } from './store.fixtures.ts';
import { t } from '../i18n/index.ts';

type Fixture = Awaited<ReturnType<typeof appRequestReviewReactFixture>>;
const press = async (f: Fixture, key: string) => {
  const control = f.controls.get(key);
  assert.ok(control, `${key}; actual controls: ${[...f.controls.keys()].join(', ')}`);
  await f.act(() => control.onPress());
};
const decisions = (f: Fixture) => f.calls.filter((c) => c.op === 'decide_tool_request');
const sent = (f: Fixture) => f.calls.filter((c) => c.op === 'request_app_edit');
const targetParams = (f: Fixture) => ({
  id: f.item.id,
  reviewRequest: f.request.id,
  reviewHash: f.request.tool_approval!.content_hash,
  reviewScope: accountScopeKey(accountScope(f.store)),
});
const image = (f: Fixture, side: string) => {
  const value = f.controls.get(`drawing-review-${side}-${f.request.id}`);
  assert.ok(value, `${side} preview must come from the real review port`);
  return value;
};
const approve = (f: Fixture) => {
  const value = f.controls.get(`label:${t('agents.allowOnce')}`);
  assert.ok(value, 'the real pending target keeps its separate approval');
  return value;
};
const focused = async (f: Fixture) => {
  await f.render();
  assert.equal(
    f.controls.has(`label:${t('agents.allowOnce')}`),
    false,
    'standalone visual review has a first-class path, not an unexplained dead approval',
  );
  await press(f, `app-request-review-${f.request.id}`);
};

test('same-owner same-head real canonical flush barrier does not invalidate the independently reviewed approval', async () => {
  const f = await appRequestReviewReactFixture({ canonicalFlush: true }),
    ack = deferred<void>();
  try {
    await focused(f);
    await f.act(() => {
      image(f, 'before').onLoad();
      image(f, 'after').onLoad();
    });
    assert.equal(approve(f).disabled, false);
    const before = f.readiness.length;
    f.holdFlush(ack.promise);
    await f.act(() => approve(f).onPress());
    assert.deepEqual(f.readiness.slice(before), [false]);
    assert.equal(
      f.controls.has('editor-agent-open'),
      false,
      'React actually observes unavailable while the real frame ACK is pending',
    );
    f.holdFlush(null);
    await f.act(() => ack.resolve());
    assert.deepEqual(
      f.readiness.slice(before),
      [false, true],
      'actual canonicalRefresh.barrier publishes the transient availability change',
    );
    assert.equal(
      f.calls.filter((c) => c.op === 'decide_tool_request').length,
      1,
      JSON.stringify({
        expected: 'one separate decision after same-head flush',
        decisionErrors: f.decisionErrors,
        readiness: f.readiness.slice(before),
      }),
    );
    assert.equal(decisions(f)[0]!.request_id, f.request.id);
    assert.equal(decisions(f)[0]!.decision, 'ApproveOnce');
    assert.deepEqual(f.decisionErrors, []);
    assert.equal(sent(f).length, 0);
    assert.deepEqual(f.errors, []);
  } finally {
    ack.resolve();
    await f.close();
  }
});

for (const change of ['device', 'canonical'] as const)
  test(`real ${change} invalidation during the flush barrier still refuses the retained approval`, async () => {
    const f = await appRequestReviewReactFixture({ canonicalFlush: true }),
      ack = deferred<void>();
    try {
      await focused(f);
      await f.act(() => {
        image(f, 'before').onLoad();
        image(f, 'after').onLoad();
      });
      assert.equal(approve(f).disabled, false);
      const retained = approve(f).onPress;
      f.holdFlush(ack.promise);
      await f.act(retained);
      assert.equal(f.readiness.at(-1), false);
      assert.equal(f.controls.has('editor-agent-open'), false);
      await f.act(() => {
        if (change === 'device') return f.reopen('device');
        f.request.tool_approval!.content_hash = 'bb'.repeat(32);
        f.item.version++;
        f.changed();
      });
      f.holdFlush(null);
      await f.act(() => ack.resolve());
      await f.act(retained);
      assert.equal(decisions(f).length, 0);
      assert.equal(sent(f).length, 0);
      assert.ok(f.decisionErrors.includes('editor_agent_owner_changed'));
      assert.deepEqual(f.errors, []);
    } finally {
      ack.resolve();
      await f.close();
    }
  });

test('actual standalone route hands existing request to the owned App, focuses only that request and reuses both genuine preview loads', async () => {
  const f = await appRequestReviewReactFixture();
  try {
    await focused(f);
    assert.equal(f.routes.length, 1);
    assert.deepEqual(f.routes[0], { pathname: '/miniapp/[id]', params: targetParams(f) });
    assert.equal(f.params.length, 1, 'route parameters are consumed once');
    assert.deepEqual(f.route().params, { id: f.item.id });
    assert.equal(f.controls.get('sheet')?.isPresented, true);
    assert.equal(f.controls.has('editor-agent-prompt'), false);
    assert.equal(f.controls.has('editor-agent-send'), false);
    assert.equal(f.controls.has(`text:${f.request.title}`), true);
    assert.equal(f.controls.has(`text:${f.sibling.title}`), false);
    assert.equal(f.renders.length, 1);
    assert.equal(f.renders[0]!.requestId, f.request.id);
    assert.equal(f.renders[0]!.contentHash, f.request.tool_approval!.content_hash);
    assert.equal(f.renders[0]!.baseVersion, f.item.version);
    assert.equal(f.calls.filter((c) => c.op === 'scene_tool_review').length, 2);
    assert.equal(sent(f).length, 0);
    assert.equal(approve(f).disabled, true);
    await f.act(() => approve(f).onPress());
    assert.equal(decisions(f).length, 0);
    const before = image(f, 'before'),
      after = image(f, 'after');
    assert.ok(before.source.uri.startsWith('data:image/png;base64,'));
    assert.notEqual(before.source.uri, after.source.uri);
    await f.act(() => before.onLoad());
    assert.equal(approve(f).disabled, true);
    await f.act(() => after.onLoad());
    assert.equal(approve(f).disabled, false);
    await f.act(() => approve(f).onPress());
    assert.equal(decisions(f).length, 1);
    assert.deepEqual(decisions(f)[0], {
      op: 'decide_tool_request',
      request_id: f.request.id,
      observed_content_hash: f.request.tool_approval!.content_hash,
      decision: 'ApproveOnce',
    });
    assert.equal(sent(f).length, 0);
    assert.equal(f.editorMounts(), 1);
    assert.equal(f.editorUnmounts(), 0);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('closing the focused sheet retires old loads/approval and canonical/readiness changes cannot reopen its consumed route', async () => {
  const f = await appRequestReviewReactFixture();
  try {
    await focused(f);
    const before = image(f, 'before'),
      after = image(f, 'after');
    await f.act(() => {
      before.onLoad();
      after.onLoad();
    });
    const oldApprove = approve(f).onPress;
    await press(f, `label:${t('common.close')}`);
    assert.equal(f.controls.get('sheet')?.isPresented, false);
    await f.act(() => {
      f.ready(false);
      f.changed();
      before.onLoad();
      after.onLoad();
    });
    await f.act(() => {
      f.ready(true);
      f.changed();
    });
    await f.act(oldApprove);
    assert.equal(f.controls.get('sheet')?.isPresented, false);
    assert.equal(f.params.length, 1);
    assert.equal(decisions(f).length, 0);
    assert.equal(sent(f).length, 0);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('failed focused PNG load requires explicit recovery and closing never resubmits or reopens it', async () => {
  const f = await appRequestReviewReactFixture();
  try {
    await focused(f);
    const before = image(f, 'before'),
      after = image(f, 'after');
    await f.act(() => before.onError({ error: 'native decode failed' }));
    await f.act(() => {
      before.onLoad();
      after.onLoad();
    });
    assert.equal(approve(f).disabled, true);
    assert.equal(f.renders.length, 1, 'failed image does not silently render again');
    await press(f, `label:${t('common.close')}`);
    await f.act(() => {
      f.ready(false);
      f.changed();
    });
    await f.act(() => {
      f.ready(true);
      f.changed();
    });
    assert.equal(f.controls.get('sheet')?.isPresented, false);
    assert.equal(f.renders.length, 1);
    assert.equal(decisions(f).length, 0);
    assert.equal(sent(f).length, 0);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

for (const change of ['epoch', 'device', 'membership'] as const)
  test(`retained standalone Review callback and its pending read cannot route after ${change} changes`, async () => {
    const f = await appRequestReviewReactFixture(),
      gate = deferred<void>();
    try {
      await f.render();
      const oldPress = f.controls.get(`app-request-review-${f.request.id}`)!.onPress;
      f.holdRequests(gate.promise);
      await f.act(oldPress);
      await f.act(() =>
        change === 'membership' ? f.revoke() : f.reopen(change === 'device' ? 'device' : undefined),
      );
      f.holdRequests(null);
      await f.act(() => gate.resolve());
      await f.act(oldPress);
      assert.equal(f.routes.length, 0);
      assert.equal(f.renders.length, 0);
      assert.equal(decisions(f).length, 0);
      assert.equal(sent(f).length, 0);
      assert.deepEqual(f.errors, []);
    } finally {
      gate.resolve();
      await f.close();
    }
  });

for (const change of [
  'foreign',
  'hash',
  'head',
  'malformed',
  'item',
  'space',
  'pin',
  'expired',
] as const)
  test(`a ${change} direct route stays inert rather than borrowing a sibling/current App`, async () => {
    const f = await appRequestReviewReactFixture();
    try {
      await f.render();
      const params: Record<string, unknown> = targetParams(f);
      if (change === 'foreign') f.request.agent.is_mine = false;
      if (change === 'hash') params.reviewHash = 'ab'.repeat(32);
      if (change === 'head') f.item.version++;
      if (change === 'malformed') params.reviewRequest = [f.request.id, f.sibling.id];
      if (change === 'item') params.id = 'another-item';
      if (change === 'space') f.request.space_id = 'another-space';
      if (change === 'pin') f.item.app!.resource_uri = 'ui://zoen/excalidraw.html';
      if (change === 'expired') {
        f.request.status = 'Stale';
        f.request.tool_approval!.expires_ms = String(Date.now() - 1);
        f.request.tool_approval!.review!.state = 'Available';
      }
      await f.act(() => f.navigate({ pathname: '/miniapp/[id]', params }));
      assert.notEqual(f.controls.get('sheet')?.isPresented, true);
      assert.equal(f.renders.length, 0);
      assert.equal(decisions(f).length, 0);
      assert.equal(sent(f).length, 0);
      assert.deepEqual(f.errors, []);
    } finally {
      await f.close();
    }
  });

for (const stop of ['blur', 'failed-head'] as const)
  test(`consumed review intent does not reopen after ${stop} while the normal head read is pending`, async () => {
    const f = await appRequestReviewReactFixture(),
      gate = deferred<void>();
    try {
      await f.render();
      f.holdHead(gate.promise);
      await f.act(() => f.navigate({ pathname: '/miniapp/[id]', params: targetParams(f) }));
      assert.equal(f.params.length, 1);
      assert.deepEqual(f.route().params, { id: f.item.id });
      if (stop === 'blur') await f.act(() => f.focus(false));
      else await f.act(() => f.invalidate());
      f.holdHead(null);
      await f.act(() => gate.resolve());
      await f.act(() => {
        f.focus(true);
        f.allow(true);
        f.ready(false);
      });
      await f.act(() => {
        f.ready(true);
        f.changed();
      });
      assert.equal(f.controls.get('sheet')?.isPresented, false);
      assert.equal(f.renders.length, 0);
      assert.equal(sent(f).length, 0);
      assert.equal(decisions(f).length, 0);
      assert.equal(f.params.length, 1);
      assert.deepEqual(f.errors, []);
    } finally {
      gate.resolve();
      await f.close();
    }
  });

test('real recovery inside an ordinary App sheet dispatches RequestReview with no Before/After load or new source', async () => {
  const f = await appRequestReviewReactFixture();
  try {
    f.request.status = 'Stale';
    f.request.tool_approval!.expires_ms = String(Date.now() - 1);
    f.request.tool_approval!.review!.state = 'Available';
    f.requests([f.request]);
    await f.render();
    await f.act(() => f.navigate({ pathname: '/miniapp/[id]', params: { id: f.item.id } }));
    await press(f, 'editor-agent-open');
    assert.equal(f.renders.length, 0);
    assert.equal(f.calls.filter((c) => c.op === 'scene_tool_review').length, 0);
    await press(f, `label:${t('reviewRenewal.renew')}`);
    assert.equal(decisions(f).length, 1);
    assert.deepEqual(decisions(f)[0], {
      op: 'decide_tool_request',
      request_id: f.request.id,
      observed_content_hash: f.request.tool_approval!.content_hash,
      decision: 'RequestReview',
    });
    assert.equal(f.renders.length, 0);
    assert.equal(sent(f).length, 0);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
