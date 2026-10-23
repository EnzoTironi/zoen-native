import assert from 'node:assert/strict';
import { test } from 'node:test';
import { DeviceSession, createDevicePort } from '../devices/session.ts';
import type { DeviceDto } from './generated/DeviceDto.ts';
import type { AccountScope } from './account-scope.ts';
import { ClientStore } from './store.ts';
import { Driver, deferred, empty } from './store.fixtures.ts';

const phone = 'a'.repeat(64),
  other = 'b'.repeat(64);
const owner: AccountScope = {
  identity: 'owner',
  device: phone,
  relay: 'https://relay.test',
  epoch: 1,
};
const rows = (): DeviceDto[] => [
  { device_id: phone, this_device: true, revoked: false },
  { device_id: other, this_device: false, revoked: false },
];
function fixture() {
  let scope: AccountScope | null = { ...owner };
  let devices = rows();
  let read = async () => devices;
  let revoke = async (id: string) => {
    devices = devices.map((row) => (row.device_id === id ? { ...row, revoked: true } : row));
  };
  const calls: string[] = [];
  const session = new DeviceSession({
    scope: () => scope,
    read: () => {
      calls.push('read');
      return read();
    },
    revoke: (id) => {
      calls.push(`revoke:${id}`);
      return revoke(id);
    },
  });
  session.activate(scope);
  return {
    session,
    calls,
    setScope(value: AccountScope | null) {
      scope = value;
    },
    setRead(value: typeof read) {
      read = value;
    },
    setRevoke(value: typeof revoke) {
      revoke = value;
    },
    activate(value = owner) {
      scope = { ...value };
      session.activate(scope);
    },
    async load() {
      await session.load(session.ticket());
    },
    select(id = other) {
      session.select(id, session.ticket());
    },
  };
}

test('the selected device is revoked once and the directory reconciles its actual revoked state', async () => {
  const f = fixture();
  await f.load();
  f.select();
  const confirmation = deferred<boolean>();
  const ticket = f.session.ticket();
  const first = f.session.revoke(other, () => confirmation.promise, ticket);
  const duplicate = f.session.revoke(
    other,
    async () => {
      throw new Error('duplicate confirmation');
    },
    ticket,
  );
  confirmation.resolve(true);
  await Promise.all([first, duplicate]);
  assert.deepEqual(f.calls, ['read', `revoke:${other}`, 'read']);
  assert.equal(f.session.getSnapshot().pending, null);
  assert.equal(
    f.session.getSnapshot().devices.find((row) => row.device_id === other)?.revoked,
    true,
  );
  assert.equal(f.session.getSnapshot().devices.find((row) => row.this_device)?.revoked, false);
});

test('late confirmations and old buttons cannot revoke after owner or focus retirement', async () => {
  for (const change of [
    { ...owner, identity: 'other-owner' },
    { ...owner, device: 'c'.repeat(64) },
    { ...owner, relay: 'https://other-relay.test' },
    { ...owner, epoch: 2 },
    null,
  ]) {
    const f = fixture();
    await f.load();
    f.select();
    const confirmation = deferred<boolean>();
    const ticket = f.session.ticket();
    const pending = f.session.revoke(other, () => confirmation.promise, ticket);
    f.setScope(change);
    confirmation.resolve(true);
    await pending;
    await f.session.revoke(other, async () => true, ticket);
    assert.deepEqual(f.calls, ['read']);
  }
  const f = fixture();
  await f.load();
  f.select();
  const confirmation = deferred<boolean>();
  const pending = f.session.revoke(other, () => confirmation.promise, f.session.ticket());
  f.session.retire();
  f.activate();
  await f.load();
  confirmation.resolve(true);
  await pending;
  assert.deepEqual(f.calls, ['read', 'read']);
});

test('late reads cannot republish a retired account directory', async () => {
  const f = fixture();
  const response = deferred<DeviceDto[]>();
  f.setRead(() => response.promise);
  const read = f.load();
  f.session.retire();
  response.resolve(rows());
  await read;
  assert.equal(f.session.getSnapshot().phase, 'inactive');
  assert.deepEqual(f.session.getSnapshot().devices, []);
});

test('lost acknowledgement reconciles a committed revocation without resending it', async () => {
  const f = fixture();
  await f.load();
  f.select();
  f.setRevoke(async () => {
    f.setRead(async () =>
      rows().map((row) => (row.device_id === other ? { ...row, revoked: true } : row)),
    );
    throw new Error('reply lost after accept');
  });
  await f.session.revoke(other, async () => true, f.session.ticket());
  assert.deepEqual(f.calls, ['read', `revoke:${other}`, 'read']);
  assert.equal(f.session.getSnapshot().revokeFailed, false);
  assert.equal(f.session.getSnapshot().devices[1]?.revoked, true);
});

test('unconfirmed revocation preserves the real active row and error without automatic mutation retry', async () => {
  const f = fixture();
  await f.load();
  f.select();
  f.setRevoke(async () => {
    throw new Error('connection refused');
  });
  await f.session.revoke(other, async () => true, f.session.ticket());
  assert.equal(f.session.getSnapshot().revokeFailed, true);
  assert.equal(f.session.getSnapshot().devices[1]?.revoked, false);
  assert.deepEqual(f.calls, ['read', `revoke:${other}`, 'read']);
});

test('cancel, sheet dismissal, own device and revoked rows cannot issue a revoke', async () => {
  const f = fixture();
  await f.load();
  f.select();
  await f.session.revoke(other, async () => false, f.session.ticket());
  const confirmation = deferred<boolean>();
  const pending = f.session.revoke(other, () => confirmation.promise, f.session.ticket());
  f.session.select(null, f.session.ticket());
  confirmation.resolve(true);
  await pending;
  f.select(phone);
  await f.session.revoke(phone, async () => true, f.session.ticket());
  f.setRead(async () => rows().map((row) => ({ ...row, revoked: !row.this_device })));
  await f.load();
  f.select();
  await f.session.revoke(other, async () => true, f.session.ticket());
  assert.ok(f.calls.every((call) => call === 'read'));
});

test('a stale completion cannot clear the replacement session pending confirmation', async () => {
  const f = fixture();
  await f.load();
  f.select();
  const old = deferred<void>();
  f.setRevoke(() => old.promise);
  const pending = f.session.revoke(other, async () => true, f.session.ticket());
  await Promise.resolve();
  f.activate({ ...owner, epoch: 2 });
  await f.load();
  f.select();
  const confirmation = deferred<boolean>();
  const current = f.session.revoke(other, () => confirmation.promise, f.session.ticket());
  old.reject(new Error('retired IO'));
  await pending;
  assert.equal(f.session.getSnapshot().pending, other);
  confirmation.resolve(false);
  await current;
  assert.equal(f.session.getSnapshot().pending, null);
});

test('duplicate, foreign-current and missing-current directories fail closed and can be refreshed', async () => {
  for (const malformed of [
    [rows()[0]!, rows()[0]!],
    [rows()[1]!],
    rows().map((row) => ({ ...row, this_device: !row.this_device })),
    [{ device_id: '', revoked: false, this_device: true }],
  ]) {
    const f = fixture();
    f.setRead(async () => malformed);
    await f.load();
    assert.equal(f.session.getSnapshot().phase, 'failed');
    assert.deepEqual(f.session.getSnapshot().devices, []);
    f.setRead(async () => rows());
    await f.load();
    assert.equal(f.session.getSnapshot().phase, 'ready');
  }
});

test('the production port uses Core directory and unlink commands under a ready account scope', async () => {
  const driver = new Driver();
  const account = {
    identity_id: owner.identity,
    device_id: phone,
    relay_url: owner.relay,
    name: 'Owner',
    handle: 'owner',
    registered: true,
    unlocked: true,
    setup_pending: false,
  };
  driver.opening = async () => ({ ...empty, account });
  const calls: unknown[] = [];
  driver.operation = async (command) => {
    calls.push(command);
    if (command.op === 'snapshot') return { ...empty, account };
    if (command.op === 'devices') return rows();
    return null;
  };
  const store = new ClientStore(driver);
  await store.open('account');
  const port = createDevicePort(store);
  assert.equal(port.scope()?.identity, owner.identity);
  assert.deepEqual(await port.read(), rows());
  await port.revoke(other);
  assert.ok(
    calls.some(
      (value) =>
        JSON.stringify(value) === JSON.stringify({ op: 'unlink_device', device_id: other }),
    ),
  );
  await store.open('demo');
  assert.equal(port.scope(), null);
});
