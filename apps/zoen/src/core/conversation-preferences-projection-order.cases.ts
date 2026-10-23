import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  conversationOwnerKey,
  parseConversationPreferences,
  projectConversationCards,
} from '../miniapps/conversation-preferences.ts';
import {
  deferred,
  failure,
  fixture,
  ids,
  miniapp,
  ownerA,
  preference,
} from './conversation-preferences.fixtures.ts';

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
