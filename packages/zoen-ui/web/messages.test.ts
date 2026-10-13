import test from 'node:test';
import assert from 'node:assert/strict';
import { Messages, retryAtMs, watchRetry, type RetryTimer } from './messages.ts';
import type { LinkedAccount, StoredSession } from './session.ts';

const account: LinkedAccount = {
  identity: 'ab'.repeat(32), device: 'cd'.repeat(32), cert: 'cert', name: 'Fixture', handle: 'fixture',
  relay: 'http://127.0.0.1:9000', primary: 'ab'.repeat(32), primary_hpke: '', hpke_secret: '',
  link_secret: '', agreement_secret: '', profile_key: '', profile_key_version: 0, profile_seen_version: 0,
  profile_fields: '',
};

function update(retryAt: number | null): string {
  return JSON.stringify({ snapshot: 'snapshot', frames: [], conversations: [], retry_at_ms: retryAt });
}

test('one retry timer follows retry_at_ms and advances at that instant', () => {
  const scheduled: { at: number; fire: () => void; cleared: boolean }[] = [];
  const advances: number[] = [];
  let now = 1_000;
  const schedule = (delay: number, fire: () => void): RetryTimer => {
    const entry = { at: now + delay, fire, cleared: false };
    scheduled.push(entry);
    return { clear() { entry.cleared = true; } };
  };
  const advance = (at: number) => { advances.push(at); };
  let timer = watchRetry(null, retryAtMs(update(5_000)), () => now, schedule, advance);
  assert.equal(scheduled.length, 1);
  const first = scheduled[0];
  assert.ok(first);
  assert.equal(first.at, 5_000);
  assert.equal(first.cleared, false);

  now = 2_000;
  timer = watchRetry(timer, retryAtMs(update(9_000)), () => now, schedule, advance);
  assert.equal(first.cleared, true);
  const second = scheduled[1];
  assert.ok(second);
  assert.equal(second.at, 9_000);
  assert.equal(second.cleared, false);

  const cleared = watchRetry(timer, retryAtMs(update(null)), () => now, schedule, advance);
  assert.equal(cleared, null);
  assert.equal(second.cleared, true);
  assert.deepEqual(advances, []);

  now = 3_000;
  watchRetry(null, retryAtMs(update(4_500)), () => now, schedule, advance);
  const third = scheduled[2];
  assert.ok(third);
  assert.equal(third.at, 4_500);
  now = 4_500;
  third.fire();
  assert.deepEqual(advances, [4_500]);
});

test('an out of date browser session clears the linked snapshot and returns to pairing', async () => {
  const saved: StoredSession[] = [];
  const states: string[] = [];
  let unpaired = 0;
  const messages = new Messages({
    core: {
      text: async () => { throw new Error('This browser session is out of date. Pair this browser again.'); },
      bytes: async () => new Uint8Array(),
      close() {},
    },
    vault: { save: async (session) => { saved.push(session); }, sign: async () => new Uint8Array(64) },
    session: { kind: 'linked', account, snapshot: '{"version":1}' },
    changed: (state) => { states.push(state.kind); },
    unpaired: () => { unpaired += 1; },
  });
  await messages.start();
  assert.deepEqual(saved, [{ kind: 'empty' }]);
  assert.equal(unpaired, 1);
  assert.deepEqual(states, []);
});

test('a different restore failure keeps the linked snapshot', async () => {
  const saved: StoredSession[] = [];
  let unpaired = 0;
  let failed = '';
  const messages = new Messages({
    core: {
      text: async () => { throw new Error('storage failed'); },
      bytes: async () => new Uint8Array(),
      close() {},
    },
    vault: { save: async (session) => { saved.push(session); }, sign: async () => new Uint8Array(64) },
    session: { kind: 'linked', account, snapshot: '{"version":2}' },
    changed: (state) => { if (state.kind === 'failed') failed = state.message; },
    unpaired: () => { unpaired += 1; },
  });
  await assert.rejects(messages.start(), /storage failed/);
  assert.deepEqual(saved, []);
  assert.equal(unpaired, 0);
  assert.equal(failed, 'storage failed');
});
