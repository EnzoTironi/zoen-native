import type { LocalProfilePhoto, ProfilePhotoDriver } from './types.ts';

type Lease = {
  refs: number;
  photo: LocalProfilePhoto | null;
  pending: Promise<LocalProfilePhoto | null>;
};
const leases = new WeakMap<ProfilePhotoDriver, Map<string, Lease>>();

/** Repeated rows share one disposable decrypted URI, never a second avatar database. */
export function acquireProfilePhoto(
  driver: ProfilePhotoDriver,
  owner: string,
  device: string,
  identity: string,
  sha: string,
) {
  let cache = leases.get(driver);
  if (!cache) {
    cache = new Map();
    leases.set(driver, cache);
  }
  const key = `${owner}:${device}:${identity}:${sha}`;
  let lease = cache.get(key);
  if (!lease) {
    lease = { refs: 0, photo: null, pending: Promise.resolve(null) };
    const created = lease;
    created.pending = driver
      .materialize(owner, device, identity, sha)
      .then((photo) => {
        created.photo = photo;
        if (photo && created.refs === 0) void driver.release(photo.uri);
        return photo;
      })
      .catch(() => null);
    cache.set(key, lease);
  }
  lease.refs++;
  const current = lease;
  let released = false;
  return {
    pending: current.pending,
    release() {
      if (released) return;
      released = true;
      current.refs--;
      if (current.refs === 0) {
        if (cache.get(key) === current) cache.delete(key);
        if (current.photo) void driver.release(current.photo.uri);
      }
    },
  };
}
