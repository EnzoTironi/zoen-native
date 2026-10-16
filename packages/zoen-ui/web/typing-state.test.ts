import test from 'node:test';
import assert from 'node:assert/strict';
import { expireTyping, parseCoreUiEvents, reduceTyping, typingPeers, TYPING_TTL_MS } from './typing-state.ts';

const me = '11'.repeat(32), other = '22'.repeat(32), third = '33'.repeat(32);
const spaces = new Set(['sp_chat']);
const typing = { kind: 'typing' as const, space: 'sp_chat', from: other, name: 'Ana' };

test('typing expires at the native six-second boundary and renewal survives the earlier deadline', () => {
  const initial = reduceTyping({}, [typing], 100, me, spaces);
  assert.equal(typingPeers(initial, 'sp_chat')[0]?.expiresAt, 100 + TYPING_TTL_MS);
  const renewed = reduceTyping(initial, [typing], 3_100, me, spaces);
  assert.equal(typingPeers(expireTyping(renewed, 6_100, spaces), 'sp_chat').length, 1);
  assert.deepEqual(expireTyping(renewed, 9_100, spaces), {});
  assert.equal(typingPeers(initial, 'sp_chat')[0]?.expiresAt, 6_100, 'old published snapshots are not mutated');
});

test('stopped removes only its peer; verified changed-space refresh clears that space', () => {
  const initial = reduceTyping({}, [typing, { ...typing, from: third, name: null }], 0, me, spaces);
  const stopped = reduceTyping(initial, [{ kind: 'stopped', space: 'sp_chat', from: other }], 1, me, spaces);
  assert.deepEqual(typingPeers(stopped, 'sp_chat').map(peer => peer.identity), [third]);
  assert.deepEqual(reduceTyping(stopped, [{ kind: 'space_changed', space: 'sp_chat' }], 2, me, spaces), {});
});

test('self and unavailable-space events cannot create typing state; removed spaces are pruned', () => {
  assert.deepEqual(reduceTyping({}, [{ ...typing, from: me }, { ...typing, space: 'unknown' }], 0, me, spaces), {});
  const initial = reduceTyping({}, [typing], 0, me, spaces);
  assert.deepEqual(expireTyping(initial, 1, new Set()), {});
});

test('Core event parser preserves real identifiers and rejects malformed event shapes', () => {
  assert.deepEqual(parseCoreUiEvents([typing]), [typing]);
  assert.deepEqual(parseCoreUiEvents(undefined), []);
  for (const value of [[{ ...typing, from: 'not-a-key' }], [{ ...typing, kind: 'counter' }], [{ ...typing, name: 3 }], [{ ...typing, space: '' }]]) {
    assert.throws(() => parseCoreUiEvents(value));
  }
});
