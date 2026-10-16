import test from 'node:test';
import assert from 'node:assert/strict';
import { LivePresence, lastSeenLabel, TYPING_TTL_MS } from './presence.ts';
import { parseFrame } from './transport.ts';

test('presence wire preserves timestamp and self-only privacy instead of collapsing to other', () => {
  assert.deepEqual(parseFrame(JSON.stringify({ kind: 'presence', identity: 'person', online: false, last_seen_ms: 123, hide_last_seen: true })),
    { kind: 'presence', identity: 'person', online: false, last_seen_ms: 123, hide_last_seen: true });
  assert.deepEqual(parseFrame(JSON.stringify({ kind: 'presence', identity: 'person', online: true })),
    { kind: 'presence', identity: 'person', online: true, last_seen_ms: null, hide_last_seen: null });
  assert.throws(() => parseFrame(JSON.stringify({ kind: 'presence', identity: 'person', online: false, last_seen_ms: -1 })));
  assert.deepEqual(parseFrame(JSON.stringify({ kind: 'ephemeral', space: 'group', from: 'person', activity: 'typing' })),
    { kind: 'ephemeral', space: 'group', from: 'person', activity: 'typing' });
});

test('concurrent typers refresh and expire independently, per conversation', (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const live = new LivePresence(() => {});
  t.after(() => live.clear());
  live.activity('group', 'Ana', true);
  live.activity('other', 'Ana', true);
  t.mock.timers.tick(2_000);
  live.activity('group', 'Bia', true);
  live.activity('group', 'Ana', true);
  assert.deepEqual(live.snapshot().typing.group, ['Ana', 'Bia']);
  t.mock.timers.tick(TYPING_TTL_MS - 2_000);
  assert.equal(live.snapshot().typing.other, undefined);
  assert.deepEqual(live.snapshot().typing.group, ['Ana', 'Bia']);
  live.activity('group', 'Ana', false);
  assert.deepEqual(live.snapshot().typing.group, ['Bia']);
  t.mock.timers.tick(2_000);
  assert.deepEqual(live.snapshot().typing, {});
});

test('hidden last seen clears old time, offline stops typing and reconnect clears all stale state', (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  let publications = 0;
  const live = new LivePresence(() => { ++publications; });
  live.presence('Ana', false, 1000);
  assert.match(lastSeenLabel(live.snapshot().people.Ana), /^Visto por último em /);
  live.presence('Ana', true, 1000);
  assert.equal(lastSeenLabel(live.snapshot().people.Ana), 'Online');
  live.activity('group', 'Ana', true);
  live.activity('group', 'Bia', true);
  live.presence('Ana', false, null);
  assert.equal(lastSeenLabel(live.snapshot().people.Ana), '');
  assert.deepEqual(live.snapshot().typing.group, ['Bia']);
  live.privacyChanged(true);
  assert.equal(live.snapshot().hideLastSeen, true);
  live.clear();
  assert.deepEqual(live.snapshot(), { people: {}, typing: {}, hideLastSeen: null });
  const before = publications;
  t.mock.timers.tick(TYPING_TTL_MS * 2);
  assert.equal(publications, before);
});
