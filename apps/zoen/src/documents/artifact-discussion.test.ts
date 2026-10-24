import assert from 'node:assert/strict';
import test from 'node:test';
import { createRequire } from 'node:module';
import { fixture } from './artifact-discussion.fixtures.ts';

const require = createRequire(import.meta.url);
const {
  StackRouter,
  StackActions,
} = require('expo-router/build/react-navigation/routers/StackRouter');

// Installed Expo58 router reducer: the component's emitted requests, not a fake
// 'push succeeded' assertion. Root layout does not configure singular thread IDs.
function threadStack(pushes: unknown[]) {
  const router = StackRouter({ initialRouteName: 'page/[id]' });
  const config = {
    routeNames: ['page/[id]', 'thread/[space]/[root]'],
    routeParamList: {},
    routeGetIdList: {},
  };
  let state = {
    stale: false,
    type: 'stack',
    key: 'test-stack',
    routeKeySeq: 0,
    index: 0,
    routeNames: config.routeNames,
    routes: [{ key: 'page', name: 'page/[id]', params: { id: 'page' } }],
  };
  for (const push of pushes as { pathname: string; params: object }[])
    state = router.getStateForAction(
      state,
      StackActions.push(push.pathname.slice(1), push.params),
      config,
    ).state;
  return { state, back: () => router.getStateForAction(state, StackActions.pop(1), config).state };
}

test('actual artifact discussion opens one canonical thread for rapid taps, and opens again after return', () => {
  const f = fixture();
  const button = f.button(f.render());
  assert.equal(button.props.label, 'documents.discussionCount:2');
  button.props.onPress();
  button.props.onPress();
  assert.equal(f.pushes.length, 1, 'two taps must not stack duplicate thread screens');
  assert.deepEqual(f.pushes[0], {
    pathname: '/thread/[space]/[root]',
    params: { space: 'space', root: 'original-root' },
  });
  const stack = threadStack(f.pushes);
  assert.equal(stack.state.routes.length, 2);
  assert.equal(stack.back().routes.length, 1, 'one Back returns directly to the same Page');
  assert.equal(stack.back().routes[0].params.id, 'page');
  assert.deepEqual(f.reads[0], { op: 'item_discussion', item_id: 'page', space_id: 'space' });
  f.blur();
  f.focus();
  f.button(f.render()).props.onPress();
  assert.equal(f.pushes.length, 2, 'returning permits a new deliberate discussion visit');
});

test('an old comments callback cannot open a thread after blur/refocus or background/foreground', () => {
  for (const lifecycle of ['navigation', 'background', 'unmount'] as const) {
    const f = fixture();
    const old = f.button(f.render()).props.onPress;
    if (lifecycle === 'navigation') {
      f.blur();
      f.focus();
    }
    if (lifecycle === 'background') {
      f.background();
      f.foreground();
    }
    if (lifecycle === 'unmount') f.retire();
    if (lifecycle !== 'unmount') f.render();
    old();
    assert.equal(f.pushes.length, 0, lifecycle);
    if (lifecycle !== 'unmount') {
      f.button(f.render()).props.onPress();
      assert.equal(f.pushes.length, 1, 'a fresh gesture remains usable');
    }
  }
});

test('cached comments still require current owner, device, session, membership and non-retiring store', () => {
  for (const loss of ['owner', 'device', 'epoch', 'membership', 'retiring'] as const) {
    const f = fixture();
    const open = f.button(f.render()).props.onPress;
    if (loss === 'owner') f.state.snapshot!.account!.identity_id = 'other';
    if (loss === 'device') f.state.snapshot!.account!.device_id = 'other';
    if (loss === 'epoch') f.store.sessionEpoch++;
    if (loss === 'membership') f.state.snapshot!.conversations[0]!.members = [];
    if (loss === 'retiring') f.store.isSessionRetiring = true;
    open();
    assert.deepEqual(f.pushes, [], loss);
  }
});

test('discussion loading, unavailable root and explicit retry do not navigate or create a comment', () => {
  const f = fixture();
  f.data(null);
  let tree = f.render();
  assert.equal(f.button(tree).props.disabled, true);
  assert.equal(f.button(tree).props.label, 'documents.loadingDiscussion');
  f.button(tree).props.onPress();
  f.error('Read unavailable');
  tree = f.render();
  assert.equal(f.button(tree).props.label, 'documents.retryDiscussion');
  f.button(tree).props.onPress();
  assert.equal(f.retries(), 1);
  f.error(null);
  f.data({ item_id: 'page', space_id: 'space', root: null });
  assert.equal(f.render(), null);
  assert.deepEqual(f.pushes, []);
});
