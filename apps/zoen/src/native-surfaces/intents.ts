import { sameSurfaceOwner, surfaceId, validSurfaceOwner } from './snapshot.ts';
import type { SnapshotItem, SurfaceOwner } from './snapshot';

export type SurfaceInvocation = Readonly<{
  id: string;
  name: string;
  params: Record<string, unknown>;
  createdAt: number;
}>;
export type SurfaceNavigation =
  | Readonly<{ kind: 'item'; owner: SurfaceOwner; itemId: string; version: number }>
  | Readonly<{ kind: 'activity'; owner: SurfaceOwner; itemId: string; linkId: string }>
  | Readonly<{ kind: 'chat'; owner: SurfaceOwner; spaceId: string }>;
export type SurfaceInvocationResult =
  | { status: 'opened'; path: string }
  | { status: 'unsupported' | 'invalid' | 'unavailable' | 'stale' | 'owner_changed' };
export interface SurfaceAccess {
  currentOwner(): SurfaceOwner | null;
  item(itemId: string): Promise<SnapshotItem>;
  canOpenSpace(spaceId: string): Promise<boolean>;
  canOpenActivity?(item: SnapshotItem, linkId: string): boolean;
  open(path: string): void;
}

/** This allowlist deliberately excludes domain mutations and provider credentials. */
export function surfaceNavigation(
  invocation: SurfaceInvocation,
  now = Date.now(),
): SurfaceNavigation | null {
  if (
    !surfaceId(invocation.id) ||
    !Number.isSafeInteger(invocation.createdAt) ||
    invocation.createdAt > now + 10_000 ||
    invocation.createdAt < now - 24 * 60 * 60_000 ||
    !invocation.params ||
    typeof invocation.params !== 'object' ||
    Array.isArray(invocation.params)
  )
    return null;
  const params = invocation.params;
  const owner = { identityId: params.identityId, deviceId: params.deviceId };
  if (
    typeof owner.identityId !== 'string' ||
    typeof owner.deviceId !== 'string' ||
    !validSurfaceOwner(owner as SurfaceOwner)
  )
    return null;
  const boundOwner: SurfaceOwner = Object.freeze(owner as SurfaceOwner);
  if (
    invocation.name === 'zoen.openActivity' &&
    Object.keys(params).sort().join(',') === 'deviceId,identityId,itemId,linkId' &&
    surfaceId(params.itemId) &&
    surfaceId(params.linkId)
  )
    return Object.freeze({
      kind: 'activity',
      owner: boundOwner,
      itemId: params.itemId,
      linkId: params.linkId,
    });
  if (
    invocation.name === 'zoen.openItem' &&
    Object.keys(params).sort().join(',') === 'deviceId,identityId,itemId,version' &&
    surfaceId(params.itemId) &&
    typeof params.version === 'number' &&
    Number.isSafeInteger(params.version) &&
    params.version >= 0
  )
    return Object.freeze({
      kind: 'item',
      owner: boundOwner,
      itemId: params.itemId,
      version: params.version,
    });
  if (
    invocation.name === 'zoen.openChat' &&
    Object.keys(params).sort().join(',') === 'deviceId,identityId,spaceId' &&
    surfaceId(params.spaceId)
  )
    return Object.freeze({ kind: 'chat', owner: boundOwner, spaceId: params.spaceId });
  return null;
}

/** Repeatable navigation only. Acknowledging the native queue belongs to the caller after success. */
export async function openSurfaceInvocation(
  invocation: SurfaceInvocation,
  access: SurfaceAccess,
  now = Date.now(),
): Promise<SurfaceInvocationResult> {
  if (!['zoen.openItem', 'zoen.openChat', 'zoen.openActivity'].includes(invocation.name))
    return { status: 'unsupported' };
  const navigation = surfaceNavigation(invocation, now);
  if (!navigation) return { status: 'invalid' };
  if (!sameSurfaceOwner(access.currentOwner(), navigation.owner))
    return { status: 'owner_changed' };
  try {
    let path: string;
    let activityItem: SnapshotItem | undefined;
    if (navigation.kind !== 'chat') {
      const item = await access.item(navigation.itemId);
      if (item.id !== navigation.itemId || !surfaceId(item.space_id))
        return { status: 'unavailable' };
      if (navigation.kind === 'item' && item.version !== navigation.version)
        return { status: 'stale' };
      if (navigation.kind === 'activity') {
        if (!access.canOpenActivity?.(item, navigation.linkId)) return { status: 'stale' };
        activityItem = item;
      }
      if (!(await access.canOpenSpace(item.space_id))) return { status: 'unavailable' };
      path = `/item/${encodeURIComponent(item.id)}`;
    } else {
      if (!(await access.canOpenSpace(navigation.spaceId))) return { status: 'unavailable' };
      path = `/chat/${encodeURIComponent(navigation.spaceId)}`;
    }
    if (!sameSurfaceOwner(access.currentOwner(), navigation.owner))
      return { status: 'owner_changed' };
    // Selection/access may change while the membership check is pending.
    if (
      navigation.kind === 'activity' &&
      (!activityItem || !access.canOpenActivity?.(activityItem, navigation.linkId))
    )
      return { status: 'stale' };
    access.open(path);
    return { status: 'opened', path };
  } catch {
    // The pending invocation is retained. Never log params/content or call this success.
    return { status: 'unavailable' };
  }
}
