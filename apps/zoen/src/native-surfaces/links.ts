import { surfaceId, validSurfaceOwner } from './snapshot.ts';
import type { SurfaceOwner } from './snapshot';
import type { SurfaceInvocation } from './intents';

export function surfaceItemURL(owner: SurfaceOwner, itemId: string, version: number): string {
  if (
    !validSurfaceOwner(owner) ||
    !surfaceId(itemId) ||
    !Number.isSafeInteger(version) ||
    version < 0
  )
    throw new Error('Invalid surface scope');
  const query = new URLSearchParams({ ...owner, version: String(version) });
  return `zoen://surface/${itemId}?${query}`;
}

/** An extension can request navigation, never a mutation or another account's content. */
export function surfaceLinkPath(value: string): string | null {
  if (value.length > 2_048) return null;
  try {
    const url = new URL(value);
    if (
      url.protocol !== 'zoen:' ||
      url.hostname !== 'surface' ||
      url.username ||
      url.password ||
      url.port ||
      url.hash
    )
      return null;
    const itemId = url.pathname.slice(1);
    const keys = [...url.searchParams.keys()].sort().join(',');
    const owner = {
      identityId: url.searchParams.get('identityId') ?? '',
      deviceId: url.searchParams.get('deviceId') ?? '',
    };
    const version = url.searchParams.get('version') ?? '';
    if (
      !surfaceId(itemId) ||
      !validSurfaceOwner(owner) ||
      keys !== 'deviceId,identityId,version' ||
      !/^(0|[1-9][0-9]{0,15})$/.test(version) ||
      !Number.isSafeInteger(Number(version))
    )
      return null;
    return `/surface?${new URLSearchParams({ ...owner, itemId, version })}`;
  } catch {
    return null;
  }
}

export function surfaceRouteInvocation(params: Record<string, unknown>): SurfaceInvocation {
  return {
    id: 'surface-navigation',
    name: 'zoen.openItem',
    createdAt: Date.now(),
    params: {
      identityId: params.identityId,
      deviceId: params.deviceId,
      itemId: params.itemId,
      version:
        typeof params.version === 'string' && /^(0|[1-9][0-9]{0,15})$/.test(params.version)
          ? Number(params.version)
          : NaN,
    },
  };
}
