import assert from 'node:assert/strict';
import { decodeWake } from '../notifications/wake.ts';
import { localNotes } from '../notifications/local.ts';
import type { Snapshot } from './types.ts';
import { test } from 'node:test';
import {
  apnsToken,
  pushConfiguration,
  PushCoordinator,
  type PushPorts,
} from '../notifications/coordinator.ts';

const configuration = { environment: 'sandbox', topic: 'xyz.tironi.zoen' } as const;

test('remote wakes reject content/actions and retain exact cursors as untrusted sync hints', () => {
  const wake = {
    aps: { 'content-available': 1 },
    zoen: { v: 1, space_id: 'sp_signed', event_seq: '18446744073709551615' },
  };
  assert.deepEqual(decodeWake(wake), {
    spaceId: 'sp_signed',
    eventSequence: '18446744073709551615',
  });
  assert.deepEqual(decodeWake({ aps: { 'content-available': 1 } }), {
    spaceId: null,
    eventSequence: null,
  });
  for (const invalid of [
    { ...wake, title: 'Untrusted content' },
    { ...wake, zoen: { ...wake.zoen, approve: true } },
    { ...wake, zoen: { ...wake.zoen, event_seq: 1 } },
    { ...wake, zoen: { ...wake.zoen, event_seq: '01' } },
    { ...wake, zoen: { ...wake.zoen, event_seq: '18446744073709551616' } },
    { ...wake, zoen: { ...wake.zoen, v: true } },
    { ...wake, aps: { 'content-available': true } },
    'x'.repeat(4097),
  ])
    assert.equal(decodeWake(invalid), null);
});

function notificationSnapshot(): Snapshot {
  const me = {
    id: 'identity',
    name: 'Enzo',
    handle: 'enzo',
    initials: 'E',
    kind: 'Person' as const,
    tint_hex: '#333333',
    glyph: null,
    bio: '',
    owner_id: null,
    owner_name: null,
    owner_tint_hex: null,
    photo_sha256: null,
    photo_ready: false,
    photo_mime: null,
    is_me: true,
    is_mine: false,
  };
  const other = { ...me, id: 'other', name: 'Marina', handle: 'marina', is_me: false };
  return {
    account: {
      identity_id: me.id,
      device_id: 'device',
      name: me.name,
      handle: me.handle,
      relay_url: 'https://relay.tryzoen.com',
      registered: true,
      unlocked: true,
      setup_pending: false,
    },
    me,
    connection: { state: 'online', synced: true, pending: '0', error: null },
    integrity_warnings: {},
    has_unreadable_local_history: false,
    requests: [],
    agents: [],
    conversations: [
      {
        id: 'sp_signed',
        title: 'Marina',
        kind: 'Direct',
        privacy: 'EndToEnd',
        members: [me, other],
        counterpart: other,
        last_preview: 'Hello',
        last_author: other,
        last_at_ms: '123',
        unread: 1,
        last_delivery: null,
        pinned: false,
        archived: false,
        muted: false,
        muted_until_ms: null,
        pending_requests: 0,
        event_count: '18446744073709551615',
      },
    ],
  };
}

test('local notifications require synchronized current-account membership, new unread content and unmuted space', () => {
  const before = notificationSnapshot();
  before.conversations[0]!.unread = 0;
  const after = notificationSnapshot();
  const note = localNotes(before, after)[0]!;
  assert.equal(note.body, 'Hello');
  assert.equal(note.sequence, '18446744073709551615');
  assert.equal(note.deviceId, 'device');
  assert.deepEqual(localNotes(after, after), []);
  for (const mutate of [
    (value: Snapshot) => {
      value.conversations[0]!.muted = true;
    },
    (value: Snapshot) => {
      value.conversations[0]!.archived = true;
    },
    (value: Snapshot) => {
      value.conversations[0]!.members = [];
    },
    (value: Snapshot) => {
      value.conversations[0]!.last_author = value.me;
    },
    (value: Snapshot) => {
      value.connection.synced = false;
    },
    (value: Snapshot) => {
      value.account!.device_id = 'other_device';
    },
  ]) {
    const value = notificationSnapshot();
    mutate(value);
    assert.deepEqual(localNotes(before, value), []);
  }
});
function fixture() {
  const writes: string[] = [];
  let current = true;
  const retries: { callback: () => void; delay: number; cancelled: boolean }[] = [];
  const ports: PushPorts = {
    async permission() {
      return 'allowed';
    },
    async token() {
      return 'AA'.repeat(32);
    },
    async register(_, token) {
      writes.push(token);
      return { kind: 'acknowledged' };
    },
    current: () => current,
    schedule(callback, delay) {
      const retry = { callback, delay, cancelled: false };
      retries.push(retry);
      return () => {
        retry.cancelled = true;
      };
    },
  };
  return {
    ports,
    writes,
    retries,
    replace: () => {
      current = false;
    },
  };
}

test('APNs inputs normalize variable tokens and require an explicit scoped environment/topic', () => {
  assert.equal(apnsToken('ABCD'), 'abcd');
  assert.equal(apnsToken('AB'.repeat(256))?.length, 512);
  for (const token of ['', 'abc', 'ab '.repeat(3), '0x12', 'aa'.repeat(257), 1])
    assert.equal(apnsToken(token), null);
  assert.deepEqual(pushConfiguration(configuration), configuration);
  for (const config of [
    { ...configuration, environment: 'development' },
    { ...configuration, topic: '*' },
    { ...configuration, topic: 'xyz..zoen' },
    {},
  ])
    assert.equal(pushConfiguration(config), null);
});

test('registration requires relay acknowledgement and token rotation cannot overtake an earlier write', async () => {
  const { ports, writes } = fixture();
  let finish: (value: { kind: 'acknowledged' }) => void = () => {};
  ports.register = async (_, token) => {
    writes.push(token);
    if (writes.length === 1)
      return new Promise((resolve) => {
        finish = resolve;
      });
    return { kind: 'acknowledged' };
  };
  const coordinator = new PushCoordinator(ports, configuration);
  const first = coordinator.foreground();
  await Promise.resolve();
  await Promise.resolve();
  assert.equal(coordinator.getSnapshot().registration, 'registering');
  const rotated = coordinator.receivedToken('BB'.repeat(32));
  assert.equal(writes.length, 1);
  finish({ kind: 'acknowledged' });
  await Promise.all([first, rotated]);
  assert.deepEqual(writes, ['aa'.repeat(32), 'bb'.repeat(32)]);
  assert.equal(coordinator.getSnapshot().registration, 'acknowledged');
  assert.equal(coordinator.getSnapshot().deliveryVerified, false);
  await coordinator.receivedToken('BB'.repeat(32));
  assert.equal(writes.length, 2);
  coordinator.dispose();
});

test('network failures are transient waiting state, with five bounded retries and no fake durable queue', async () => {
  const { ports, retries } = fixture();
  ports.register = async () => {
    throw new Error('offline');
  };
  const coordinator = new PushCoordinator(ports, configuration);
  await coordinator.foreground();
  assert.equal(coordinator.getSnapshot().registration, 'waiting');
  for (let i = 0; i < 5; i++) {
    retries[i]?.callback();
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  }
  assert.deepEqual(
    retries.map((retry) => retry.delay),
    [10_000, 20_000, 40_000, 80_000, 160_000],
  );
  coordinator.dispose();
  assert.equal(retries.at(-1)?.cancelled, false); // Last retry already ran; no timer remains.
});

test('denied permission removes the server token, while stale permission callbacks cannot register another account', async () => {
  const { ports, writes, replace } = fixture();
  const coordinator = new PushCoordinator(ports, configuration);
  ports.permission = async () => 'denied';
  await coordinator.foreground();
  assert.deepEqual(writes, ['']);
  assert.equal(coordinator.getSnapshot().registration, 'unregistered');
  let finish: (permission: 'allowed') => void = () => {};
  ports.permission = () =>
    new Promise((resolve) => {
      finish = resolve;
    });
  const stale = coordinator.foreground(true);
  replace();
  finish('allowed');
  await stale;
  assert.deepEqual(writes, ['']);
  coordinator.dispose();
});

test('unsupported build and explicit server refusal remain truthful terminal states', async () => {
  const { ports, writes } = fixture();
  const unsupported = new PushCoordinator(ports, null);
  await unsupported.foreground(true);
  assert.equal(unsupported.getSnapshot().registration, 'unsupported');
  assert.equal(writes.length, 0);
  unsupported.dispose();
  ports.register = async () => ({ kind: 'refused' });
  const refused = new PushCoordinator(ports, configuration);
  await refused.foreground();
  assert.equal(refused.getSnapshot().registration, 'refused');
  refused.dispose();
});
