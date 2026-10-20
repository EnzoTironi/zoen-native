import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { HomeOwner, HomePreferencesPort } from '../home/preferences';
import {
  ConversationCardsController,
  ConversationCardsFailure,
  conversationOwnerKey,
  parseConversationPreferences,
  projectConversationCards,
} from '../miniapps/conversation-preferences.ts';
import type { ItemDetail } from './generated/ItemDetail';

const ownerA: HomeOwner = { mode: 'account', identityId: 'alice', deviceId: 'phone_a' };
const ownerB: HomeOwner = { mode: 'account', identityId: 'bruno', deviceId: 'phone_b' };
const failure = (kind: ConversationCardsFailure['kind']) => (error: unknown) =>
  error instanceof ConversationCardsFailure && error.kind === kind;
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
function miniapp(id: string, space = 'shared'): ItemDetail {
  return {
    id,
    space_id: space,
    title: `Canonical private title ${id}`,
    app: { app_id: 'poll', snapshot_json: '{"private":"canonical only"}' },
  } as ItemDetail;
}
function fixture() {
  let owner: HomeOwner | null = ownerA;
  let spaces = new Set(['shared', 'second']);
  let items = [miniapp('it_one'), miniapp('it_two'), miniapp('it_other', 'second')];
  const saved = new Map<string, string>();
  const port: HomePreferencesPort = {
    get: async (key) => saved.get(key) ?? null,
    set: async (key, value) => {
      saved.set(key, value);
    },
  };
  const source = { owner: () => owner, spaces: () => spaces, items: () => items };
  const cards = new ConversationCardsController(source, port);
  return {
    cards,
    source,
    port,
    saved,
    owner: (value: HomeOwner | null) => {
      owner = value;
    },
    spaces: (value: string[]) => {
      spaces = new Set(value);
    },
    items: (value: ItemDetail[]) => {
      items = value;
    },
  };
}
const ids = (items: readonly ItemDetail[]) => items.map((item) => item.id);
const preference = (hiddenIds: string[] = [], orderedIds: string[] = []) => ({
  hiddenIds,
  orderedIds,
});

test('a queued same-set drag cannot overwrite an accepted conversation reorder', async () => {
  const f = fixture();
  await f.cards.activate();
  const pending = deferred<void>();
  const originalSet = f.port.set;
  let writes = 0;
  f.port.set = async (key, value) => {
    ++writes;
    await pending.promise;
    await originalSet(key, value);
  };
  const expected = ['it_one', 'it_two'];
  const first = f.cards.reorder('shared', ['it_two', 'it_one'], ownerA, expected);
  const stale = assert.rejects(
    f.cards.reorder('shared', ['it_one', 'it_two'], ownerA, expected),
    failure('save'),
  );
  expected.reverse();
  await Promise.resolve();
  assert.equal(writes, 1);
  pending.resolve();
  await Promise.all([first, stale]);
  assert.equal(writes, 1);
  assert.deepEqual(ids(f.cards.cards('shared').visible), ['it_two', 'it_one']);
  assert.deepEqual(ids(f.cards.cards('second').visible), ['it_other']);
  assert.deepEqual(JSON.parse(f.saved.get(conversationOwnerKey(ownerA)!)!), {
    shared: preference([], ['it_two', 'it_one']),
  });
  await f.cards.reorder('shared', ['it_one', 'it_two'], ownerA, ['it_two', 'it_one']);
  assert.equal(writes, 2);
  assert.deepEqual(ids(f.cards.cards('shared').visible), ['it_one', 'it_two']);
});

test('rails default to all canonical apps, hide every card recoverably, and never modify Home favorites or Items', async () => {
  const f = fixture();
  const canonical = f.source.items();
  f.saved.set('home-pins:1:unrelated', '["it_two"]');
  await f.cards.activate();
  assert.deepEqual(ids(f.cards.cards('shared').visible), ['it_one', 'it_two']);
  await f.cards.setPinned('shared', 'it_one', false);
  await f.cards.setPinned('shared', 'it_two', false);
  assert.deepEqual(f.cards.cards('shared').visible, []);
  assert.deepEqual(ids(f.cards.cards('shared').all), ['it_one', 'it_two']);
  assert.deepEqual(f.cards.cards('shared').hiddenIds, ['it_one', 'it_two']);
  assert.equal(f.source.items(), canonical);
  assert.equal(f.saved.get('home-pins:1:unrelated'), '["it_two"]');
  const stored = f.saved.get(conversationOwnerKey(ownerA)!)!;
  assert.deepEqual(JSON.parse(stored), { shared: preference(['it_one', 'it_two']) });
  assert.equal(stored.includes('Canonical'), false);
  assert.equal(stored.includes('snapshot'), false);
  f.cards.dispose();
  await f.cards.activate();
  assert.deepEqual(f.cards.cards('shared').visible, []);
  await f.cards.setPinned('shared', 'it_two', true);
  assert.deepEqual(ids(f.cards.cards('shared').visible), ['it_two']);
});

test('preference parsing rejects content, prototype keys, malformed structures and bounded excess', () => {
  const parsed = parseConversationPreferences(
    JSON.stringify({ shared: preference(['it_one', 'it_one'], ['it_two', 'it_two']) }),
  );
  assert.equal(Object.getPrototypeOf(parsed), null);
  assert.deepEqual(parsed.shared, preference(['it_one'], ['it_two']));
  for (const value of [
    null,
    '',
    'null',
    '[]',
    '1',
    '{',
    '{"__proto__":{"hiddenIds":[],"orderedIds":[]}}',
    '{"constructor":{"hiddenIds":[],"orderedIds":[]}}',
    JSON.stringify({ '../private': preference() }),
    JSON.stringify({ shared: { hiddenIds: ['it_one'], orderedIds: [], title: 'Private' } }),
    JSON.stringify({ shared: { hiddenIds: [], orderedIds: [{ id: 'it_one' }] } }),
    JSON.stringify({ shared: { hiddenIds: [], orderedIds: ['../private'] } }),
    JSON.stringify({ shared: { hiddenIds: [], orderedIds: ['x'.repeat(129)] } }),
    JSON.stringify({ shared: { hiddenIds: [], orderedIds: Array(101).fill('it_one') } }),
    JSON.stringify(
      Object.fromEntries(Array.from({ length: 101 }, (_, i) => [`space_${i}`, preference()])),
    ),
    ' '.repeat(1_048_577),
  ])
    assert.deepEqual(Object.keys(parseConversationPreferences(value)), []);
  assert.notEqual(
    conversationOwnerKey(ownerA),
    conversationOwnerKey({ ...ownerA, deviceId: 'other' }),
  );
  assert.notEqual(conversationOwnerKey(ownerA), conversationOwnerKey({ ...ownerA, mode: 'demo' }));
  assert.equal(conversationOwnerKey(null), null);
});

test('projection excludes cross-chat, non-app, duplicate and inaccessible Items, retaining canonical references', () => {
  const first = miniapp('it_one');
  const result = projectConversationCards(
    [
      miniapp('it_other', 'second'),
      first,
      miniapp('it_one'),
      miniapp('it_two'),
      { ...miniapp('it_note'), app: null },
      miniapp('../private'),
    ],
    new Set(['shared']),
    'shared',
    preference(['it_two'], ['it_other', 'it_two', 'it_one']),
  );
  assert.deepEqual(ids(result.all), ['it_two', 'it_one']);
  assert.deepEqual(ids(result.visible), ['it_one']);
  assert.equal(result.visible[0], first);
  assert.deepEqual(projectConversationCards([first], new Set(), 'shared').all, []);
});

test('reordering retains hidden and unavailable slots, appends new apps, and restores their saved position when available', async () => {
  const f = fixture();
  f.saved.set(
    conversationOwnerKey(ownerA)!,
    JSON.stringify({
      shared: preference(
        ['it_hidden', 'hidden_unavailable'],
        ['unavailable', 'it_one', 'it_hidden', 'it_two', 'other_unavailable'],
      ),
    }),
  );
  f.items([miniapp('it_one'), miniapp('it_two'), miniapp('it_hidden'), miniapp('it_new')]);
  await f.cards.activate();
  await f.cards.reorder('shared', ['it_new', 'it_two', 'it_one']);
  assert.deepEqual(
    f.cards.getSnapshot().preferences.shared,
    preference(
      ['it_hidden', 'hidden_unavailable'],
      ['unavailable', 'it_new', 'it_hidden', 'it_two', 'other_unavailable', 'it_one'],
    ),
  );
  f.items([...f.source.items(), miniapp('unavailable'), miniapp('it_newer')]);
  await f.cards.refresh();
  assert.deepEqual(ids(f.cards.cards('shared').visible), [
    'unavailable',
    'it_new',
    'it_two',
    'it_one',
    'it_newer',
  ]);
  await f.cards.setPinned('shared', 'it_hidden', true);
  assert.deepEqual(ids(f.cards.cards('shared').visible), [
    'unavailable',
    'it_new',
    'it_hidden',
    'it_two',
    'it_one',
    'it_newer',
  ]);
});

test('independent chat selections survive reload without leaking hidden or ordered IDs into another chat', async () => {
  const f = fixture();
  f.items([...f.source.items(), miniapp('it_second', 'second')]);
  await f.cards.activate();
  await f.cards.setPinned('shared', 'it_one', false);
  await f.cards.reorder('second', ['it_second', 'it_other']);
  f.cards.dispose();
  await f.cards.activate();
  assert.deepEqual(ids(f.cards.cards('shared').visible), ['it_two']);
  assert.deepEqual(ids(f.cards.cards('second').visible), ['it_second', 'it_other']);
  await assert.rejects(f.cards.setPinned('shared', 'it_other', false), failure('save'));
  assert.deepEqual(f.cards.cards('second').hiddenIds, []);
});

test('pending restore cannot be overwritten, and a failed read is retryable without displaying hidden cards', async () => {
  const f = fixture();
  const restore = deferred<string | null>();
  f.port.get = () => restore.promise;
  const active = f.cards.activate();
  assert.equal(f.cards.getSnapshot().loading, true);
  assert.deepEqual(f.cards.cards('shared').all, []);
  await assert.rejects(f.cards.setPinned('shared', 'it_one', false), failure('save'));
  await assert.rejects(f.cards.reorder('shared', ['it_two', 'it_one']), failure('save'));
  assert.equal(f.saved.size, 0);
  restore.resolve(JSON.stringify({ shared: preference(['it_one']) }));
  await active;
  assert.deepEqual(ids(f.cards.cards('shared').visible), ['it_two']);

  const retry = fixture();
  retry.port.get = async () => {
    throw new Error('Read unavailable');
  };
  await retry.cards.activate();
  assert.equal(retry.cards.getSnapshot().error?.kind, 'load');
  assert.deepEqual(retry.cards.cards('shared').all, []);
  retry.port.get = async () => JSON.stringify({ shared: preference(['it_two']) });
  await retry.cards.refresh();
  assert.equal(retry.cards.getSnapshot().error, null);
  assert.deepEqual(ids(retry.cards.cards('shared').visible), ['it_one']);
});

test('malformed, partial, duplicate, hidden and cross-chat reorder requests fail without writing', async () => {
  const f = fixture();
  await f.cards.activate();
  let writes = 0;
  f.port.set = async () => {
    ++writes;
  };
  for (const order of [
    [],
    ['it_one'],
    ['it_one', 'it_one'],
    ['it_one', 'it_other'],
    ['it_one', '../private'],
    Array(101).fill('it_one') as string[],
  ])
    await assert.rejects(f.cards.reorder('shared', order), failure('save'));
  await f.cards.reorder('shared', ['it_one', 'it_two']);
  await f.cards.setPinned('shared', 'it_two', true);
  assert.equal(writes, 0);
  await f.cards.setPinned('shared', 'it_one', false);
  await assert.rejects(f.cards.reorder('shared', ['it_one', 'it_two']), failure('save'));
  assert.equal(writes, 1);
});

test('queued writes copy caller arrays, serialize committed order and retain the selection after save failure', async () => {
  const f = fixture();
  f.items([miniapp('it_one'), miniapp('it_two'), miniapp('it_three')]);
  await f.cards.activate();
  const pending = deferred<void>();
  let writes = 0;
  f.port.set = async (key, value) => {
    if (++writes === 1) await pending.promise;
    f.saved.set(key, value);
  };
  const first = f.cards.reorder('shared', ['it_three', 'it_one', 'it_two']);
  const requested = ['it_two', 'it_three', 'it_one'];
  const second = f.cards.reorder('shared', requested);
  requested.fill('it_one');
  const hide = f.cards.setPinned('shared', 'it_three', false);
  await Promise.resolve();
  assert.equal(writes, 1);
  assert.deepEqual(ids(f.cards.cards('shared').visible), ['it_one', 'it_two', 'it_three']);
  pending.resolve();
  await Promise.all([first, second, hide]);
  assert.deepEqual(ids(f.cards.cards('shared').visible), ['it_two', 'it_one']);
  const confirmed = f.saved.get(conversationOwnerKey(ownerA)!);
  const original = f.port.set;
  f.port.set = async () => {
    throw new Error('Storage unavailable');
  };
  await assert.rejects(f.cards.reorder('shared', ['it_one', 'it_two']), failure('save'));
  assert.deepEqual(ids(f.cards.cards('shared').visible), ['it_two', 'it_one']);
  assert.equal(f.saved.get(conversationOwnerKey(ownerA)!), confirmed);
  f.port.set = original;
  await f.cards.reorder('shared', ['it_one', 'it_two']);
  assert.equal(f.cards.getSnapshot().error, null);
  assert.deepEqual(ids(f.cards.cards('shared').visible), ['it_one', 'it_two']);
});

test('queued pin changes invalidate stale orders and preserve newly visible apps', async () => {
  const f = fixture();
  await f.cards.activate();
  const hide = f.cards.setPinned('shared', 'it_one', false);
  const stale = assert.rejects(f.cards.reorder('shared', ['it_two', 'it_one']), failure('save'));
  await Promise.all([hide, stale]);
  const show = f.cards.setPinned('shared', 'it_one', true);
  const incomplete = assert.rejects(f.cards.reorder('shared', ['it_two']), failure('save'));
  await Promise.all([show, incomplete]);
  f.items([...f.source.items(), miniapp('it_new')]);
  await assert.rejects(f.cards.reorder('shared', ['it_two', 'it_one']), failure('save'));
  assert.deepEqual(ids(f.cards.cards('shared').visible), ['it_one', 'it_two', 'it_new']);
});

test('actual membership and canonical item membership are checked when queued work runs, without requiring refresh', async () => {
  const f = fixture();
  await f.cards.activate();
  const pending = deferred<void>();
  const original = f.port.set;
  let writes = 0;
  f.port.set = async (key, value) => {
    ++writes;
    await pending.promise;
    await original(key, value);
  };
  const first = f.cards.setPinned('second', 'it_other', false);
  const hide = assert.rejects(f.cards.setPinned('shared', 'it_one', false), failure('save'));
  const reorder = assert.rejects(f.cards.reorder('shared', ['it_two', 'it_one']), failure('save'));
  await Promise.resolve();
  f.spaces(['second']);
  assert.deepEqual(f.cards.cards('shared').all, []);
  pending.resolve();
  await Promise.all([first, hide, reorder]);
  assert.equal(writes, 1);
  f.spaces(['shared', 'second']);
  const removed = f.cards.setPinned('shared', 'it_one', false);
  f.items([miniapp('it_one', 'second'), miniapp('it_two')]);
  await assert.rejects(removed, failure('save'));
  assert.equal(writes, 1);
});

test('owner changes fence late restores, delayed menu callbacks and in-flight writes, including same-owner return', async () => {
  const f = fixture();
  const restore = deferred<string | null>();
  f.port.get = (key) =>
    key === conversationOwnerKey(ownerA) ? restore.promise : Promise.resolve(null);
  const old = f.cards.activate();
  const oldOwner = f.cards.getSnapshot().owner;
  f.owner(ownerB);
  await f.cards.refresh();
  restore.resolve(JSON.stringify({ shared: preference(['it_one']) }));
  await old;
  assert.deepEqual(f.cards.getSnapshot().owner, ownerB);
  assert.deepEqual(ids(f.cards.cards('shared').visible), ['it_one', 'it_two']);
  await assert.rejects(
    f.cards.setPinned('shared', 'it_one', false, oldOwner),
    failure('accountChanged'),
  );

  f.port.get = async (key) => f.saved.get(key) ?? null;
  const write = deferred<void>();
  const original = f.port.set;
  f.port.set = async (key, value) => {
    await write.promise;
    await original(key, value);
  };
  const oldWrite = f.cards.setPinned('shared', 'it_one', false);
  const refusal = assert.rejects(oldWrite, failure('accountChanged'));
  await Promise.resolve();
  f.owner(null);
  await f.cards.refresh();
  f.owner(ownerB);
  const returning = f.cards.refresh();
  assert.equal(f.cards.getSnapshot().loading, true);
  assert.deepEqual(f.cards.cards('shared').all, []);
  write.resolve();
  await Promise.all([refusal, returning]);
  assert.deepEqual(ids(f.cards.cards('shared').visible), ['it_two']);
  assert.equal(f.cards.getSnapshot().error, null);
  assert.deepEqual(JSON.parse(f.saved.get(conversationOwnerKey(ownerB)!)!), {
    shared: preference(['it_one']),
  });
});

test('a slow previous-owner write does not block restoration for an unrelated owner', async () => {
  const f = fixture();
  await f.cards.activate();
  const pending = deferred<void>();
  f.port.set = async (key, value) => {
    await pending.promise;
    f.saved.set(key, value);
  };
  const old = assert.rejects(
    f.cards.setPinned('shared', 'it_one', false),
    failure('accountChanged'),
  );
  await Promise.resolve();
  f.saved.set(conversationOwnerKey(ownerB)!, JSON.stringify({ shared: preference(['it_two']) }));
  f.owner(ownerB);
  await f.cards.refresh();
  assert.deepEqual(ids(f.cards.cards('shared').visible), ['it_one']);
  pending.resolve();
  await old;
  assert.deepEqual(ids(f.cards.cards('shared').visible), ['it_one']);
});

test('account, device and demo scopes reject stale callbacks even when they share the same canonical IDs', async () => {
  const f = fixture();
  await f.cards.activate();
  for (const owner of [
    { ...ownerA, deviceId: 'phone_other' },
    { ...ownerA, mode: 'demo' as const },
    ownerB,
  ]) {
    const captured = f.cards.getSnapshot().owner;
    f.owner(owner);
    assert.deepEqual(f.cards.cards('shared').all, []);
    await f.cards.refresh();
    await assert.rejects(
      f.cards.setPinned('shared', 'it_one', false, captured),
      failure('accountChanged'),
    );
    await assert.rejects(
      f.cards.reorder('shared', ['it_two', 'it_one'], captured),
      failure('accountChanged'),
    );
  }
  assert.equal(f.saved.size, 0);
});

test('erase drains pending writes, fences late restores and clears only the captured device preference key', async () => {
  const f = fixture();
  f.saved.set(conversationOwnerKey(ownerB)!, JSON.stringify({ shared: preference(['it_two']) }));
  await f.cards.activate();
  const pending = deferred<void>();
  const original = f.port.set;
  let first = true;
  f.port.set = async (key, value) => {
    if (first) {
      first = false;
      await pending.promise;
    }
    await original(key, value);
  };
  const hide = assert.rejects(
    f.cards.setPinned('shared', 'it_one', false),
    failure('accountChanged'),
  );
  await Promise.resolve();
  f.owner(null);
  await f.cards.refresh();
  f.cards.dispose();
  const erase = f.cards.eraseDevice();
  pending.resolve();
  await Promise.all([hide, erase]);
  assert.equal(f.saved.get(conversationOwnerKey(ownerA)!), '');
  assert.deepEqual(JSON.parse(f.saved.get(conversationOwnerKey(ownerB)!)!), {
    shared: preference(['it_two']),
  });
  assert.deepEqual(f.cards.cards('shared').all, []);

  const late = fixture();
  const restore = deferred<string | null>();
  late.port.get = () => restore.promise;
  const active = late.cards.activate();
  late.owner(null);
  await late.cards.eraseDevice();
  restore.resolve(JSON.stringify({ shared: preference(['it_one']) }));
  await active;
  assert.deepEqual(Object.keys(late.cards.getSnapshot().preferences), []);
  assert.equal(late.saved.get(conversationOwnerKey(ownerA)!), '');
});

test('failed erasure preserves cleanup ownership and blocks every new write until erasure retry succeeds', async () => {
  const f = fixture();
  await f.cards.activate();
  await f.cards.setPinned('shared', 'it_one', false);
  f.owner(null);
  await f.cards.refresh();
  const original = f.port.set;
  f.port.set = async () => {
    throw new Error('Cannot clear preferences');
  };
  await assert.rejects(f.cards.eraseDevice(), /Cannot clear preferences/);
  await assert.rejects(f.cards.setPinned('shared', 'it_two', false), failure('accountChanged'));
  f.port.set = original;
  await f.cards.eraseDevice();
  assert.equal(f.saved.get(conversationOwnerKey(ownerA)!), '');
});

test('demo choices remain volatile and disposed pending work cannot publish after remount', async () => {
  const f = fixture();
  f.owner({ ...ownerA, mode: 'demo' });
  let reads = 0;
  f.port.get = async () => {
    ++reads;
    return JSON.stringify({ shared: preference(['it_one']) });
  };
  await f.cards.activate();
  await f.cards.setPinned('shared', 'it_one', false);
  await f.cards.reorder('shared', ['it_two']);
  assert.equal(reads, 0);
  assert.equal(f.saved.size, 0);
  f.cards.dispose();
  await f.cards.activate();
  assert.deepEqual(ids(f.cards.cards('shared').visible), ['it_one', 'it_two']);

  const late = fixture();
  const restore = deferred<string | null>();
  late.port.get = () => restore.promise;
  const old = late.cards.activate();
  late.cards.dispose();
  late.port.get = async () => JSON.stringify({ shared: preference(['it_two']) });
  await late.cards.activate();
  restore.resolve(JSON.stringify({ shared: preference(['it_one']) }));
  await old;
  assert.deepEqual(ids(late.cards.cards('shared').visible), ['it_one']);
});

test('preference map and ID limits fail safely without truncating a committed chat selection', async () => {
  const f = fixture();
  f.saved.set(
    conversationOwnerKey(ownerA)!,
    JSON.stringify(
      Object.fromEntries(
        Array.from({ length: 100 }, (_, i) => [`space_${i}`, preference(['it_hidden'])]),
      ),
    ),
  );
  await f.cards.activate();
  await assert.rejects(f.cards.setPinned('shared', 'it_one', false), failure('save'));
  assert.equal(Object.keys(f.cards.getSnapshot().preferences).length, 100);
  assert.equal(f.cards.getSnapshot().preferences.shared, undefined);
  assert.deepEqual(ids(f.cards.cards('shared').visible), ['it_one', 'it_two']);
});

test('typing-only refreshes preserve snapshot identity while canonical and access changes notify subscribers', async () => {
  const f = fixture();
  await f.cards.activate();
  const initial = f.cards.getSnapshot();
  let notifications = 0;
  f.cards.subscribe(() => {
    ++notifications;
  });
  // The source may allocate equivalent membership sets on every ClientStore update.
  f.source.spaces = () => new Set(['shared', 'second']);
  await Promise.all([f.cards.refresh(), f.cards.refresh(), f.cards.refresh()]);
  assert.equal(notifications, 0);
  assert.equal(f.cards.getSnapshot(), initial);
  f.items([...f.source.items(), miniapp('it_new')]);
  await f.cards.refresh();
  assert.equal(notifications, 1);
  assert.deepEqual(ids(f.cards.cards('shared').visible), ['it_one', 'it_two', 'it_new']);
  f.source.spaces = () => new Set(['second']);
  await f.cards.refresh();
  assert.equal(notifications, 2);
  assert.deepEqual(f.cards.cards('shared').all, []);
});
