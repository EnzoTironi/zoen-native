import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  callElapsed,
  readCallPresentation,
  watchCallPresentation,
  type CallPresentation,
} from '../calls/presentation.ts';

const owner = { identity: 'alice', device: 'a1' };
const current = {
  identity_id: 'alice',
  device_id: 'a1',
  space_id: 'dm',
  call_id: `call_${'a'.repeat(32)}`,
  state: 'connected',
  minimized: true,
  connected_at_ms: 1000,
};
const encode = (value: unknown) => JSON.stringify(value);

test('return presentation requires exact account/device and a valid transient call shape', () => {
  assert.deepEqual(readCallPresentation(encode(current), owner), current);
  for (const change of [
    { identity_id: 'bob' },
    { device_id: 'other' },
    { call_id: 'wrong' },
    { state: 'ended' },
    { state: { toString: {} } },
    { minimized: 'true' },
    { connected_at_ms: Number.MAX_SAFE_INTEGER + 1 },
  ])
    assert.equal(readCallPresentation(encode({ ...current, ...change }), owner), null);
  for (const json of ['broken', 'null', 'x'.repeat(2049)])
    assert.equal(readCallPresentation(json, owner), null);
  assert.equal(readCallPresentation(encode(current), { identity: undefined, device: 'a1' }), null);
  assert.equal(callElapsed(null, 4000), null);
  assert.equal(callElapsed(1000, 65_000), '1:04');
  assert.equal(callElapsed(1000, 0), '0:00');
});

test('slow initial native query cannot revive a call ended by a newer callback', async () => {
  const values: Array<CallPresentation | null> = [];
  let event!: (json: string) => void;
  let resolve!: (json: string) => void;
  let removed = 0;
  const stop = watchCallPresentation({
    listen(callback) {
      event = callback;
      return () => {
        removed += 1;
      };
    },
    current: () => new Promise<string>((ready) => (resolve = ready)),
    owner: () => owner,
    changed: (value) => values.push(value),
  });
  event(encode(current));
  event(encode({ ...current, state: 'ended', minimized: false }));
  resolve(encode(current));
  await Promise.resolve();
  assert.deepEqual(values, [current, null]);
  stop();
  event(encode(current));
  assert.equal(values.length, 2);
  assert.equal(removed, 1);
});

test('disposed query and an account changed during the read cannot publish old call UI', async () => {
  const values: Array<CallPresentation | null> = [];
  let resolve!: (json: string) => void;
  let currentOwner = owner;
  const stop = watchCallPresentation({
    listen: () => () => {},
    current: () => new Promise<string>((ready) => (resolve = ready)),
    owner: () => currentOwner,
    changed: (value) => values.push(value),
  });
  currentOwner = { identity: 'bob', device: 'b1' };
  resolve(encode(current));
  await Promise.resolve();
  assert.equal(values.length, 1);
  assert.equal(values[0], null);
  stop();
  let late!: (json: string) => void;
  const dispose = watchCallPresentation({
    listen: () => () => {},
    current: () => new Promise<string>((ready) => (late = ready)),
    owner: () => owner,
    changed: (value) => values.push(value),
  });
  dispose();
  late(encode(current));
  await Promise.resolve();
  assert.deepEqual(values, [null]);
});
