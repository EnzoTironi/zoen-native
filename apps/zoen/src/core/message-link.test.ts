import assert from 'node:assert/strict';
import { test } from 'node:test';
import { ClientStore } from './store.ts';
import { deferred, Driver, draftSession } from './store.fixtures.ts';
import { resolveMessageLink } from './message-link.ts';
import type { Command, Snapshot, TimelineEntry } from './types';

async function fixture() {
  const driver = new Driver();
  let snapshot = draftSession();
  driver.opening = async () => snapshot;
  const calls: Command[] = [];
  let read = async () => ({ id: 'message', deleted: false, in_thread: null }) as TimelineEntry;
  driver.operation = async (command) => {
    if (command.op === 'snapshot') return snapshot;
    calls.push(command);
    assert.equal(
      command.op,
      'message',
      'Resolving a link has no publication, join or approval port',
    );
    return read();
  };
  const store = new ClientStore(driver);
  await store.open('account');
  const space = snapshot.conversations[0]!.id;
  return {
    store,
    calls,
    space,
    read: (value: typeof read) => {
      read = value;
    },
    update: async (change: (value: Snapshot) => Snapshot) => {
      snapshot = change(snapshot);
      await store.refresh();
    },
    resolve: (active = () => true) => resolveMessageLink(store, space, 'message', active),
  };
}

test('the canonical message chooses chat or thread; links cannot supply a different thread', async () => {
  const f = await fixture();
  try {
    assert.deepEqual(await f.resolve(), {
      pathname: '/chat/[id]',
      params: { id: f.space, jump: 'message' },
    });
    f.read(async () => ({ id: 'message', deleted: false, in_thread: 'root' }) as TimelineEntry);
    assert.deepEqual(await f.resolve(), {
      pathname: '/thread/[space]/[root]',
      params: { space: f.space, root: 'root', jump: 'message' },
    });
    assert.deepEqual(
      f.calls,
      Array(2).fill({ op: 'message', space_id: f.space, message_id: 'message' }),
    );
  } finally {
    await f.store.close();
  }
});

test('malformed IDs and unavailable membership never request private history', async () => {
  const f = await fixture();
  try {
    assert.equal(await resolveMessageLink(f.store, f.space, 'x/y', () => true), null);
    await f.update((state) => ({ ...state, conversations: [] }));
    assert.equal(await f.resolve(), null);
    assert.deepEqual(f.calls, []);
  } finally {
    await f.store.close();
  }
});

test('deleted, substituted and invalid thread targets cannot open unrelated content', async () => {
  const f = await fixture();
  try {
    for (const entry of [
      { id: 'message', deleted: true },
      { id: 'other', deleted: false },
      { id: 'message', deleted: false, in_thread: '../escape' },
    ]) {
      f.read(async () => entry as TimelineEntry);
      await assert.rejects(f.resolve(), /unavailable/);
    }
    f.read(async () => {
      throw new Error('missing local history');
    });
    await assert.rejects(f.resolve(), /missing local history/);
  } finally {
    await f.store.close();
  }
});

test('focus loss, membership revocation and same-account restart retire pending message links', async () => {
  for (const loss of ['focus', 'membership', 'session', 'lock'] as const) {
    const f = await fixture();
    try {
      const wait = deferred<TimelineEntry>();
      f.read(() => wait.promise);
      let active = true;
      const pending = f.resolve(() => active);
      if (loss === 'focus') active = false;
      if (loss === 'session') {
        await f.store.close();
        await f.store.open('account');
      }
      if (loss === 'membership' || loss === 'lock') {
        let original!: Snapshot;
        await f.update((state) => {
          original = state;
          return loss === 'membership'
            ? { ...state, conversations: [] }
            : { ...state, account: { ...state.account!, unlocked: false } };
        });
        await f.update(() => original);
      }
      wait.resolve({ id: 'message', deleted: false, in_thread: null } as TimelineEntry);
      assert.equal(await pending, null, loss);
    } finally {
      await f.store.close();
    }
  }
});
