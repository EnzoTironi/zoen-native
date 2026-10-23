import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createCallAudioHandoff } from '../calls/audio-handoff.ts';

const first = '11111111-1111-1111-1111-111111111111';
const second = '22222222-2222-2222-2222-222222222222';
const event = (action: string, ticket = first) => JSON.stringify({ action, ticket });

function fixture() {
  const effects: string[] = [];
  let owner: string | undefined;
  let held: (() => Promise<void>) | undefined;
  const handler = createCallAudioHandoff({
    begin(ticket) {
      owner = ticket;
      effects.push(`begin:${ticket}`);
    },
    owns: (ticket) => owner === ticket,
    end(ticket) {
      owner = undefined;
      effects.push(`end:${ticket}`);
    },
    pause: () => effects.push('pause'),
    async stopCapture() {
      effects.push('stop');
      await held?.();
    },
    async drain() {
      effects.push('drain');
    },
    async deactivateExpo() {
      effects.push('deactivate');
    },
    async reply(ticket, success) {
      effects.push(`reply:${ticket}:${success}`);
    },
  });
  return { handler, effects, hold: (operation: () => Promise<void>) => (held = operation) };
}

test('cold call adoption fences JS media without deactivating the live CallKit session', async () => {
  const { handler, effects } = fixture();
  await handler(event('adopt'));
  assert.deepEqual(effects, [`begin:${first}`, 'pause', 'stop', 'drain', `reply:${first}:true`]);
  await handler(event('release', second));
  assert.equal(effects.includes(`end:${first}`), false);
  await handler(event('release'));
  assert.deepEqual(effects.slice(-2), ['pause', `end:${first}`]);
});

test('outgoing handoff drains capture and modes before deactivation and acknowledgment', async () => {
  const { handler, effects } = fixture();
  await handler(event('acquire'));
  assert.deepEqual(effects, [
    `begin:${first}`,
    'pause',
    'stop',
    'drain',
    'deactivate',
    `reply:${first}:true`,
  ]);
});

test('retained recorder cleanup serializes a queued release behind the native fence', async () => {
  const { handler, effects, hold } = fixture();
  let finish!: () => void;
  let reached!: () => void;
  const entered = new Promise<void>((resolve) => (reached = resolve));
  hold(() => {
    reached();
    return new Promise<void>((resolve) => (finish = resolve));
  });
  const adopting = handler(event('adopt'));
  await entered;
  const releasing = handler(event('release'));
  assert.deepEqual(effects, [`begin:${first}`, 'pause', 'stop']);
  finish();
  await Promise.all([adopting, releasing]);
  assert.deepEqual(effects.slice(-3), [`reply:${first}:true`, 'pause', `end:${first}`]);
});

test('failed capture cleanup refuses adoption and malformed events have no effects', async () => {
  const { handler, effects, hold } = fixture();
  for (const json of [
    'broken',
    '{}',
    event('other'),
    event('adopt', 'not-a-ticket'),
    'x'.repeat(513),
  ])
    await handler(json);
  assert.deepEqual(effects, []);
  hold(async () => {
    throw new Error('Capture cleanup failed');
  });
  await handler(event('adopt'));
  assert.deepEqual(effects, [`begin:${first}`, 'pause', 'stop', `reply:${first}:false`]);
});
