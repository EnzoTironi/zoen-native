import { t } from '../i18n/index.ts';
import { getBrowserMediaHost, type BrowserMediaHost } from '../core/browser-media.ts';
import { MAX_PROFILE_PHOTO_BYTES, type ProfilePhotoDriver } from './types.ts';

export function createBrowserProfilePhotoDriver(
  resolveHost = getBrowserMediaHost,
): ProfilePhotoDriver {
  const owned = new Map<string, BrowserMediaHost>();
  const observed = new WeakSet<BrowserMediaHost>();
  function current(host: BrowserMediaHost, ownerId: string, deviceId: string) {
    if (
      host.closed.aborted ||
      resolveHost() !== host ||
      host.ownerId !== ownerId ||
      host.deviceId !== deviceId
    )
      throw new Error(t('profile.sessionChanged'));
  }
  function observe(host: BrowserMediaHost) {
    if (observed.has(host)) return;
    observed.add(host);
    host.closed.addEventListener(
      'abort',
      () => {
        for (const [uri, owner] of owned)
          if (owner === host) {
            URL.revokeObjectURL(uri);
            owned.delete(uri);
          }
      },
      { once: true },
    );
  }
  return {
    async save(profile) {
      const host = resolveHost();
      current(host, profile.ownerId, profile.deviceId);
      let bytes = new Uint8Array();
      if (profile.photo === 'set') {
        // Only our locally encoded picker output crosses this bridge; no remote URL.
        const url = new URL(profile.uri ?? '');
        if (url.protocol !== 'blob:' || url.origin !== window.location.origin)
          throw new Error(t('profile.chooseAgain'));
        const response = await fetch(url, { signal: host.closed });
        if (!response.ok || !response.body) throw new Error(t('profile.openPhotoFailed'));
        const reader = response.body.getReader();
        const chunks: Uint8Array[] = [];
        let length = 0;
        try {
          for (;;) {
            const { done, value } = await reader.read();
            if (done) break;
            if (value.byteLength > MAX_PROFILE_PHOTO_BYTES - length)
              throw new Error(t('profile.photoLimit'));
            length += value.byteLength;
            chunks.push(value);
          }
        } finally {
          await reader.cancel().catch(() => {});
        }
        if (!length) throw new Error(t('profile.photoEmpty'));
        bytes = new Uint8Array(length);
        let offset = 0;
        for (const chunk of chunks) {
          bytes.set(chunk, offset);
          offset += chunk.length;
        }
      } else if (profile.uri) throw new Error(t('profile.invalidPhotoChange'));
      current(host, profile.ownerId, profile.deviceId);
      await host.saveProfile(
        {
          name: profile.name,
          bio: profile.bio,
          photo: profile.photo,
          ...(profile.photo === 'set' ? { mime: 'image/jpeg' } : {}),
        },
        bytes,
      );
      current(host, profile.ownerId, profile.deviceId);
    },
    async materialize(ownerId, deviceId, identityId, expectedSha256) {
      const host = resolveHost();
      current(host, ownerId, deviceId);
      observe(host);
      const photo = await host.profilePhoto(identityId, expectedSha256);
      current(host, ownerId, deviceId);
      if (
        photo.sha256 !== expectedSha256 ||
        !photo.bytes.length ||
        photo.bytes.length > MAX_PROFILE_PHOTO_BYTES ||
        !['image/jpeg', 'image/png', 'image/heic', 'image/webp'].includes(photo.mime)
      )
        throw new Error(t('profile.photoMismatch'));
      const uri = URL.createObjectURL(
        new Blob([Uint8Array.from(photo.bytes)], { type: photo.mime }),
      );
      owned.set(uri, host);
      return { uri, identityId, sha256: expectedSha256, mime: photo.mime };
    },
    async release(uri) {
      if (owned.delete(uri)) URL.revokeObjectURL(uri);
    },
  };
}
