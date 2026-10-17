import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createBrowserProfilePhotoDriver } from '../profile/driver.web.ts';
import type { BrowserMediaHost } from './browser-media.ts';

function fixture() {
  const closed = new AbortController();
  const saves: unknown[] = [];
  const host = {
    ownerId: 'owner',
    deviceId: 'device',
    closed: closed.signal,
    async saveProfile(request: unknown, bytes: Uint8Array) {
      saves.push({ request, bytes });
    },
    async profilePhoto() {
      return { bytes: new Uint8Array([1, 2, 3]), mime: 'image/jpeg', sha256: 'ab'.repeat(32) };
    },
  } as unknown as BrowserMediaHost;
  return { host, closed, saves };
}

test('profile saves reject remote URIs and stale account/device before bytes enter Rust', async () => {
  const { host, saves } = fixture();
  const driver = createBrowserProfilePhotoDriver(() => host);
  const base = {
    ownerId: 'owner',
    deviceId: 'device',
    name: 'Ana',
    bio: 'Private',
    photo: 'set' as const,
  };
  await assert.rejects(
    driver.save({ ...base, uri: 'https://example.test/avatar.jpg' }),
    /Select the photo again/,
  );
  await assert.rejects(driver.save({ ...base, ownerId: 'other' }), /session changed/);
  await assert.rejects(driver.save({ ...base, deviceId: 'other' }), /session changed/);
  assert.equal(saves.length, 0);
  await driver.save({ ...base, photo: 'remove' });
  assert.equal(saves.length, 1);
});

test('profile previews release on close and late prior-session results create no URI', async (t) => {
  const { host, closed } = fixture();
  const urls: string[] = [];
  const created = t.mock.method(URL, 'createObjectURL', () => 'blob:owned-preview');
  t.mock.method(URL, 'revokeObjectURL', (uri: string) => {
    urls.push(uri);
  });
  const driver = createBrowserProfilePhotoDriver(() => host);
  const sha = 'ab'.repeat(32);
  const result = await driver.materialize('owner', 'device', 'person', sha);
  assert.equal(result?.uri, 'blob:owned-preview');
  closed.abort();
  assert.deepEqual(urls, ['blob:owned-preview']);
  await driver.release('blob:owned-preview');
  assert.equal(urls.length, 1);
  const next = fixture();
  let finish: ((photo: { bytes: Uint8Array; mime: string; sha256: string }) => void) | undefined;
  next.host.profilePhoto = () =>
    new Promise((resolve) => {
      finish = resolve;
    });
  const lateDriver = createBrowserProfilePhotoDriver(() => next.host);
  const pending = lateDriver.materialize('owner', 'device', 'person', sha);
  next.closed.abort();
  finish!({ bytes: new Uint8Array([1]), mime: 'image/jpeg', sha256: sha });
  await assert.rejects(pending, /session changed/);
  assert.equal(created.mock.callCount(), 1);
});

test('profile preview refuses unexpected hashes and over-limit binary responses', async () => {
  const { host } = fixture();
  const driver = createBrowserProfilePhotoDriver(() => host);
  await assert.rejects(
    driver.materialize('owner', 'device', 'person', 'cd'.repeat(32)),
    /photo does not match the profile/,
  );
  host.profilePhoto = async () => ({
    bytes: new Uint8Array(5 * 1024 * 1024 + 1),
    mime: 'image/jpeg',
    sha256: 'ab'.repeat(32),
  });
  await assert.rejects(
    driver.materialize('owner', 'device', 'person', 'ab'.repeat(32)),
    /photo does not match the profile/,
  );
});

test('repeated avatar rows share one preview and release only after the last row leaves', async () => {
  const { acquireProfilePhoto } = await import('../profile/leases.ts');
  const released: string[] = [];
  let reads = 0;
  const driver = {
    async save() {},
    async materialize() {
      reads++;
      return { uri: 'local:photo', identityId: 'person', sha256: 'hash', mime: 'image/jpeg' };
    },
    async release(uri: string) {
      released.push(uri);
    },
  };
  const first = acquireProfilePhoto(driver, 'owner', 'device', 'person', 'hash');
  const second = acquireProfilePhoto(driver, 'owner', 'device', 'person', 'hash');
  await Promise.all([first.pending, second.pending]);
  assert.equal(reads, 1);
  first.release();
  assert.deepEqual(released, []);
  second.release();
  second.release();
  assert.deepEqual(released, ['local:photo']);
});

test('a preview arriving after every row unmounts is immediately released', async () => {
  const { acquireProfilePhoto } = await import('../profile/leases.ts');
  let finish:
    | ((photo: { uri: string; identityId: string; sha256: string; mime: string }) => void)
    | undefined;
  const released: string[] = [];
  const driver = {
    async save() {},
    materialize: () =>
      new Promise<{ uri: string; identityId: string; sha256: string; mime: string }>((resolve) => {
        finish = resolve;
      }),
    async release(uri: string) {
      released.push(uri);
    },
  };
  const lease = acquireProfilePhoto(driver, 'owner', 'device', 'person', 'hash');
  lease.release();
  finish!({ uri: 'local:late', identityId: 'person', sha256: 'hash', mime: 'image/jpeg' });
  await lease.pending;
  assert.deepEqual(released, ['local:late']);
});
