import test from 'node:test';
import assert from 'node:assert/strict';
import type { ClientStore } from '../core/store';
import type { ItemDetail } from '../core/types';
import {
  deckVersionBinding,
  observeDeckVersion,
  selectedDeckHead,
} from './deck-version-selection.ts';
import { dto, original } from './canonical-binary.fixtures.ts';

function fixture() {
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  const state = {
    phase: 'ready',
    mode: 'account',
    snapshot: {
      account: { identity_id: 'human', device_id: 'phone', unlocked: true, setup_pending: false },
      me: { id: 'human' },
      conversations: [{ id: 'space', members: [{ id: 'human' }] }],
    },
  };
  const calls: unknown[] = [];
  const store = {
    sessionEpoch: 1,
    getSnapshot: () => state,
    driver: {
      request: async (command: unknown) => {
        calls.push(command);
        await held;
        return dto(original);
      },
    },
  } as unknown as ClientStore;
  const binding = deckVersionBinding(store, {
    id: 'it_deck',
    space_id: 'space',
    version: 1,
    app: { app_id: 'deckcraft' },
  } as ItemDetail)!;
  return { state, store, binding, calls, release };
}
test('history capture asks Core for the exact observed version, independently of current metadata', async () => {
  const f = fixture();
  const result = observeDeckVersion(f.store, f.binding, () => true);
  f.release();
  const selected = await result;
  assert.deepEqual(f.calls, [
    { op: 'app_binary_read', item_id: 'it_deck', version: 1, expected_sha256: null },
  ]);
  assert.equal(selected?.version, '1');
  assert.equal(selected?.sha256, dto(original).sha256);
});
for (const change of ['focus', 'account', 'device', 'membership', 'erase', 'epoch'] as const) {
  test(`late historical capture cannot navigate after ${change} retirement`, async () => {
    const f = fixture();
    let active = true;
    const result = observeDeckVersion(f.store, f.binding, () => active);
    if (change === 'focus') active = false;
    if (change === 'account') f.state.snapshot.account.identity_id = 'replacement';
    if (change === 'device') f.state.snapshot.account.device_id = 'new-device';
    if (change === 'membership') f.state.snapshot.conversations[0]!.members = [];
    if (change === 'erase') Object.assign(f.state, { erasePending: true });
    if (change === 'epoch') Object.assign(f.store, { sessionEpoch: 2 });
    f.release();
    assert.equal(await result, null);
  });
}
test('partial/malformed historical route binding never falls back to current', () => {
  for (const version of [undefined, '', '0', '-1', '01', '4294967296', ['1']])
    assert.equal(selectedDeckHead(version, 'a'.repeat(64)), null);
  for (const sha of [undefined, '', 'a'.repeat(63), 'A'.repeat(64), ['a'.repeat(64)]])
    assert.equal(selectedDeckHead('1', sha), null);
  assert.deepEqual(selectedDeckHead('1', 'a'.repeat(64)), { version: '1', sha256: 'a'.repeat(64) });
});
