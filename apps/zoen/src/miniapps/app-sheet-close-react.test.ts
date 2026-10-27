import assert from 'node:assert/strict';
import { test } from 'node:test';
import { appSheetCloseFixture } from './app-sheet-close-react.fixtures.ts';

test('compiled manage X closes directly to its Library origin with exact root source/target and no document write', async () => {
  const f = await appSheetCloseFixture({ manage: true });
  try {
    assert.ok(f.qa.button, 'management has the same native close intent');
    assert.equal(f.qa.button.name, 'xmark');
    assert.equal(f.qa.button.size, 44);
    const retained = f.qa.button.onPress;
    await f.act(retained);
    assert.equal(f.current().name, 'library');
    assert.deepEqual(f.flushes, []);
    await f.act(retained);
    assert.deepEqual(f.qa.dispatched, [
      { type: 'GO_BACK', source: 'app-modal', target: 'root-stack' },
    ]);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('nested manage X still waits for the actual canvas removal guard; failure retains it and explicit retry flushes once', async () => {
  const f = await appSheetCloseFixture({ manage: true, canvas: true, origin: 'chat' });
  try {
    assert.ok(f.qa.button);
    await f.act(() => f.qa.button.onPress());
    assert.equal(f.current().name, 'miniapp');
    assert.deepEqual(f.flushes, [{ retire: true }]);
    await f.act(() => f.qa.button.onPress());
    assert.equal(f.flushes.length, 1);
    await f.act(() => f.pending[0]!.reject(Error('controlled persistence failure')));
    assert.equal(f.current().name, 'miniapp');
    assert.equal(f.qa.disposed, 0);
    assert.ok(f.qa.labels.has('drawing.closeFailed'));
    await f.act(() => f.qa.button.onPress());
    assert.equal(f.flushes.length, 2);
    await f.act(() => f.pending[1]!.resolve());
    assert.equal(f.current().name, 'chat');
    assert.equal(f.qa.disposed, 1);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('canvas X retains the same native exclusion and existing flush guard', async () => {
  const f = await appSheetCloseFixture();
  try {
    assert.deepEqual(f.qa.chrome, { top: 32, right: 12, width: 44, height: 44 });
    await f.act(() => f.qa.button.onPress());
    assert.equal(f.current().name, 'miniapp');
    assert.deepEqual(f.flushes, [{ retire: true }]);
    await f.act(() => f.pending[0]!.resolve());
    assert.equal(f.current().name, 'library');
  } finally {
    await f.close();
  }
});

test('retained manage callbacks cannot close tool, Library, replaced or refocused routes', async () => {
  const f = await appSheetCloseFixture({ manage: true });
  try {
    assert.ok(f.qa.button);
    const stale = f.qa.button.onPress;
    await f.act(() => f.path('/miniapp/drawing/tool'));
    assert.equal(f.qa.button, null);
    await f.act(stale);
    assert.deepEqual(f.qa.dispatched, []);
    await f.act(() => f.path('/library'));
    assert.equal(f.qa.button, null);
    await f.act(stale);
    await f.act(() => f.path('/miniapp/other/manage'));
    assert.ok(f.qa.button);
    await f.act(stale);
    const blur = f.qa.button.onPress;
    await f.act(() => {
      f.focus(false);
      f.focus(true);
    });
    await f.act(blur);
    assert.deepEqual(f.qa.dispatched, []);
    assert.deepEqual(f.flushes, []);
    await f.act(() => f.qa.button.onPress());
    assert.equal(f.current().name, 'library');
  } finally {
    await f.close();
  }
});

test('manage X refuses missing state, root mismatch, lost focus and absent back destination', async () => {
  const f = await appSheetCloseFixture({ manage: true });
  try {
    assert.ok(f.qa.button);
    const read = f.qa.navigation.getState,
      back = f.qa.navigation.canGoBack;
    f.qa.navigation.getState = () => undefined;
    await f.act(() => f.qa.button.onPress());
    f.qa.navigation.getState = read;
    f.qa.routeIdentity.key = 'other';
    await f.act(() => f.qa.button.onPress());
    f.qa.routeIdentity.key = 'app-modal';
    await f.act(() => f.focus(false));
    await f.act(() => f.qa.button.onPress());
    await f.act(() => f.focus(true));
    f.qa.navigation.canGoBack = () => false;
    await f.act(() => f.qa.button.onPress());
    f.qa.navigation.canGoBack = back;
    assert.deepEqual(f.qa.dispatched, []);
    assert.deepEqual(f.flushes, []);
    assert.equal(f.current().name, 'miniapp');
  } finally {
    await f.close();
  }
});
