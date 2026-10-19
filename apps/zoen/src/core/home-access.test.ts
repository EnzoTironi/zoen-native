import assert from 'node:assert/strict';
import { test } from 'node:test';
import { homeOwnerFor, homeSpacesFor } from '../home/access.ts';
import type { Snapshot } from './types';

function session() {
  return {
    phase: 'ready' as const,
    mode: 'account' as const,
    erasePending: false,
    snapshot: {
      me: { id: 'alice' },
      account: { identity_id: 'alice', device_id: 'phone', unlocked: true, setup_pending: false },
      conversations: [
        { id: 'member', members: [{ id: 'alice', is_me: false }] },
        { id: 'cached', members: [{ id: 'bob', is_me: true }] },
      ],
    } as unknown as Snapshot,
  };
}

test('Home and conversation card access follows actual identity membership rather than cached chats or hints', () => {
  const state = session();
  assert.deepEqual([...homeSpacesFor(state)], ['member']);
  state.snapshot.conversations[0]!.members = [];
  assert.deepEqual([...homeSpacesFor(state)], []);
  state.snapshot.me = null;
  state.snapshot.conversations[0]!.members = [
    { id: 'alice', is_me: false } as Snapshot['conversations'][number]['members'][number],
  ];
  assert.deepEqual([...homeSpacesFor(state)], ['member']);
  state.snapshot.me = { id: 'bob' } as Snapshot['me'];
  assert.deepEqual([...homeSpacesFor(state)], []);
});

test('locked, setup, opening and erase sessions cannot expose cached presentation cards', () => {
  for (const change of ['locked', 'setup', 'opening', 'erasing'] as const) {
    const state: Parameters<typeof homeSpacesFor>[0] = session();
    if (change === 'locked') state.snapshot!.account!.unlocked = false;
    if (change === 'setup') state.snapshot!.account!.setup_pending = true;
    if (change === 'opening') state.phase = 'opening';
    if (change === 'erasing') state.erasePending = true;
    assert.equal(homeOwnerFor(state), null);
    assert.deepEqual([...homeSpacesFor(state)], []);
  }
});

test('demo card scope stays separate from account and uses exact demo membership', () => {
  const state: Parameters<typeof homeSpacesFor>[0] = session();
  state.mode = 'demo';
  assert.deepEqual(homeOwnerFor(state), { mode: 'demo', identityId: 'alice', deviceId: 'demo' });
  assert.deepEqual([...homeSpacesFor(state)], ['member']);
});
