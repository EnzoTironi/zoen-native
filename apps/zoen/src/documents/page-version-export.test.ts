import assert from 'node:assert/strict';
import test from 'node:test';
import type { ClientState, ClientStore } from '../core/store';
import type { PageDto } from '../core/generated/PageDto';
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
  const store = {
    sessionEpoch: 1,
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
  f.store.sessionEpoch++;
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
    if (condition === 'epoch') f.store.sessionEpoch++;
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
