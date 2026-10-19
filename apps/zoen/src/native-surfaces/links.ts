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

/** Activity taps follow their selected session, while widget/shortcut links remain version-bound. */
export function surfaceActivityURL(owner: SurfaceOwner, itemId: string, linkId: string): string {
  if (!validSurfaceOwner(owner) || !surfaceId(itemId) || !surfaceId(linkId))
    throw new Error('Invalid activity scope');
  return `zoen://activity/${itemId}?${new URLSearchParams({ ...owner, linkId })}`;
}

/** An extension can request navigation, never a mutation or another account's content. */
export function surfaceLinkPath(value: string): string | null {
  if (value.length > 2_048) return null;
  try {
    const url = new URL(value);
    if (
      url.protocol !== 'zoen:' ||
      !['surface', 'activity'].includes(url.hostname) ||
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
    if (!surfaceId(itemId) || !validSurfaceOwner(owner)) return null;
    if (url.hostname === 'activity') {
      const linkId = url.searchParams.get('linkId');
      if (keys !== 'deviceId,identityId,linkId' || !surfaceId(linkId)) return null;
      return `/surface?${new URLSearchParams({ ...owner, itemId, linkId })}`;
    }
    const version = url.searchParams.get('version') ?? '';
    if (
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
  const activity = Object.hasOwn(params, 'linkId');
  const expected = activity
    ? 'deviceId,identityId,itemId,linkId'
    : 'deviceId,identityId,itemId,version';
  return {
    id: 'surface-navigation',
    name: activity ? 'zoen.openActivity' : 'zoen.openItem',
    createdAt: Date.now(),
    params:
      Object.keys(params).sort().join(',') !== expected
        ? {}
        : {
            identityId: params.identityId,
            deviceId: params.deviceId,
            itemId: params.itemId,
            ...(activity
              ? { linkId: params.linkId }
              : {
                  version:
                    typeof params.version === 'string' &&
                    /^(0|[1-9][0-9]{0,15})$/.test(params.version)
                      ? Number(params.version)
                      : NaN,
                }),
          },
  };
}
