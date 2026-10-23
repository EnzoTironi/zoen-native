import assert from 'node:assert/strict';
import test from 'node:test';
import type { ClientState, ClientStore } from '../core/store';
import type { PageDto } from '../core/generated/PageDto';
import { DocumentExportActivity } from './export-activity.ts';
import { exportObservedPage, pageExportBinding } from './page-version-export.ts';

function fixture() {
  const state = {
    phase: 'ready',
    mode: 'account',
    erasePending: false,
    snapshot: {
      account: { identity_id: 'alice', device_id: 'phone', unlocked: true, setup_pending: false },
      me: { id: 'alice' },
      conversations: [{ id: 'chat', members: [{ id: 'alice' }] }],
    },
  } as unknown as ClientState;
  const page = {
    item_id: 'page',
    space_id: 'chat',
    version: 2,
    ready: true,
    content_hash: 'observed',
  } as PageDto;
  const result = {
    item_id: 'page',
    space_id: 'chat',
    version: 2,
    content_hash: 'observed',
    title: 'Observed title',
    markdown: '# Shared\r\n',
  };
  let resolve!: (value: typeof result) => void;
  const promise = new Promise<typeof result>((done) => {
    resolve = done;
  });
  const commands: unknown[] = [];
  let epoch = 1;
  const store = {
    get sessionEpoch() {
      return epoch;
    },
    getSnapshot: () => state,
    driver: {
      request: async (command: unknown) => {
        commands.push(command);
        return promise;
      },
    },
  } as unknown as Pick<ClientStore, 'getSnapshot' | 'driver' | 'sessionEpoch'>;
  const binding = pageExportBinding(store, page)!;
  let active = true;
  const shares: unknown[][] = [];
  const run = () =>
    exportObservedPage(
      store,
      binding,
      () => active,
      async (...args) => {
        shares.push(args);
      },
    );
  return {
    state,
    advanceEpoch: () => {
      epoch++;
    },
    page,
    store,
    result,
    commands,
    shares,
    run,
    resolve: (value = result) => resolve(value),
    dispose: () => {
      active = false;
    },
  };
}

test('selected Page export sends the exact observed binding and shares its version with a live authority fence', async () => {
  const f = fixture();
  const running = f.run();
  assert.deepEqual(f.commands, [
    {
      op: 'page_export_version',
      space_id: 'chat',
      item_id: 'page',
      version: 2,
      observed_hash: 'observed',
    },
  ]);
  f.resolve();
  assert.equal(await running, true);
  assert.deepEqual(f.shares[0]!.slice(0, 2), ['Observed title v2', '# Shared\r\n']);
  const current = f.shares[0]![2] as () => boolean;
  assert.equal(current(), true);
  f.advanceEpoch();
  assert.equal(current(), false);
});

test('late completion cannot share across owner, device, membership, route or locked-session changes', async () => {
  for (const condition of [
    'owner',
    'device',
    'epoch',
    'membership',
    'route',
    'locked',
    'setup',
    'erase',
    'phase',
    'mode',
  ]) {
    const f = fixture();
    const running = f.run();
    if (condition === 'owner') f.state.snapshot!.account!.identity_id = 'bob';
    if (condition === 'device') f.state.snapshot!.account!.device_id = 'other';
    if (condition === 'epoch') f.advanceEpoch();
    if (condition === 'membership') f.state.snapshot!.conversations[0]!.members = [];
    if (condition === 'route') f.dispose();
    if (condition === 'locked') f.state.snapshot!.account!.unlocked = false;
    if (condition === 'setup') f.state.snapshot!.account!.setup_pending = true;
    if (condition === 'erase') f.state.erasePending = true;
    if (condition === 'phase') f.state.phase = 'opening';
    if (condition === 'mode') f.state.mode = 'demo';
    f.resolve();
    assert.equal(await running, false, condition);
    assert.equal(f.shares.length, 0, condition);
    assert.equal(await f.run(), false, `retired activation ${condition}`);
    assert.equal(f.commands.length, 1, condition);
  }
});

test('response substitution fails closed without sharing or retrying the read', async () => {
  for (const [key, value] of [
    ['item_id', 'foreign'],
    ['space_id', 'foreign'],
    ['version', 3],
    ['content_hash', 'changed'],
    ['markdown', null],
    ['title', null],
  ] as const) {
    const f = fixture();
    const running = f.run();
    f.resolve({ ...f.result, [key]: value } as typeof f.result);
    await assert.rejects(running);
    assert.equal(f.shares.length, 0);
    assert.equal(f.commands.length, 1);
  }
  const f = fixture();
  assert.equal(pageExportBinding(f.store, { ...f.page, ready: false }), null);
  assert.equal(pageExportBinding(f.store, { ...f.page, version: 0 }), null);
  assert.equal(pageExportBinding(f.store, { ...f.page, space_id: 'foreign' }), null);
});

test('blur without unmount retires a held export and refocus cannot revive it or clear a newer operation', async () => {
  const f = fixture();
  const activity = new DocumentExportActivity();
  assert.equal(activity.begin(), null);
  const blur = activity.focus();
  const first = activity.begin()!;
  assert.equal(activity.begin(), null, 'double activation is fenced synchronously');
  const running = exportObservedPage(
    f.store,
    pageExportBinding(f.store, f.page)!,
    first.current,
    async () => {
      f.shares.push(['unexpected old share']);
    },
  );
  blur();
  const blurAgain = activity.focus();
  const second = activity.begin()!;
  assert.equal(first.current(), false);
  assert.equal(first.finish(), false, 'old completion cannot finish the new activation');
  assert.equal(second.current(), true);
  f.resolve();
  assert.equal(await running, false);
  assert.equal(f.shares.length, 0);
  assert.equal(second.finish(), true);
  const third = activity.begin()!;
  blur();
  assert.equal(third.current(), true, 'obsolete focus cleanup cannot retire a newer focus');
  blurAgain();
  assert.equal(third.current(), false);
});
