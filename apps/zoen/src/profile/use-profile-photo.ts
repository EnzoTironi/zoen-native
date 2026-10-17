import { useEffect, useState } from 'react';
import { useClient } from '../core/provider';
import { acquireProfilePhoto } from './leases';

export function useProfilePhoto(
  identityId: string | undefined,
  sha256: string | null | undefined,
  ready: boolean,
) {
  const { store, snapshot } = useClient();
  const owner = snapshot?.account?.identity_id;
  const device = snapshot?.account?.device_id;
  const photos = store.driver.photos;
  const [value, setValue] = useState<{ key: string; uri: string } | null>(null);
  const key = `${owner}:${device}:${identityId}:${sha256}:${ready}`;
  useEffect(() => {
    if (!photos || !owner || !device || !identityId || !sha256) return;
    let active = true;
    const lease = acquireProfilePhoto(photos, owner, device, identityId, sha256);
    void lease.pending.then((photo) => {
      if (active && photo) setValue({ key, uri: photo.uri });
    });
    return () => {
      active = false;
      lease.release();
    };
  }, [photos, owner, device, identityId, sha256, key]);
  return value?.key === key ? value.uri : undefined;
}
