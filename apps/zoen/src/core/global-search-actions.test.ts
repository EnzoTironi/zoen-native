import assert from 'node:assert/strict';
import { test } from 'node:test';
import { GlobalSearchActions } from './global-search-actions.ts';
import { deferred } from './store.fixtures.ts';

test('one focused search attempt owns navigation and cannot resume after blur or access ABA', async () => {
  for (const retire of ['blur', 'access'] as const) {
    let current = true;
    const actions = new GlobalSearchActions(() => current, 'query');
    actions.focus();
    const read = deferred<string>();
    const completions: string[] = [];
    const pending = actions.run(
      () => read.promise,
      (value) => completions.push(value),
      'failed',
    );
    let duplicate = false;
    await actions.run(
      async () => {
        duplicate = true;
      },
      () => {},
      'failed',
    );
    assert.equal(duplicate, false);
    if (retire === 'blur') actions.blur();
    else {
      current = false;
      actions.refresh();
      current = true;
    }
    read.resolve('old');
    await pending;
    assert.equal(completions.length, 0);
    actions.focus();
    await actions.run(
      async () => 'new',
      (value) => completions.push(value),
      'failed',
    );
    assert.deepEqual(completions, ['new']);
  }
});

test('late rejection cannot replace a newer attempt; current failure remains retryable', async () => {
  const actions = new GlobalSearchActions(() => true);
  actions.focus();
  const old = deferred<void>();
  const pending = actions.run(
    () => old.promise,
    () => {},
    'failed',
  );
  actions.blur();
  actions.focus();
  const next = deferred<void>();
  const current = actions.run(
    () => next.promise,
    () => {},
    'failed',
  );
  old.reject(new Error('stale private error'));
  await pending;
  assert.deepEqual(actions.getSnapshot(), { pending: true, error: null });
  next.reject(new Error('current failure'));
  await current;
  assert.deepEqual(actions.getSnapshot(), { pending: false, error: 'current failure' });
  actions.clearError();
  assert.equal(actions.getSnapshot().error, null);
  await actions.run(
    async () => {},
    () => {},
    'failed',
  );
  assert.deepEqual(actions.getSnapshot(), { pending: false, error: null });
});
