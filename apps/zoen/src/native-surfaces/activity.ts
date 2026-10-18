import { t } from '../i18n/index.ts';
import { miniappSnapshot, widgetProjection } from './snapshot.ts';
import type { NativeWidgetContent, SnapshotItem, SurfaceOwner } from './snapshot';

export type MiniappActivityContent = NativeWidgetContent & {
  endsAt: number;
  labels?: Readonly<{ refresh: string; countdown: string; inProgress: string }>;
};
export const MAX_ACTIVITY_MS = 8 * 60 * 60_000;

/** A Live Activity needs an actual canonical countdown with a bounded end, not a generic pin. */
export function activityContent(
  item: SnapshotItem,
  owner: SurfaceOwner,
  showContent = false,
  now = Date.now(),
): MiniappActivityContent | null {
  const snapshot = miniappSnapshot(item);
  if (
    snapshot?.template !== 'countdown' ||
    snapshot.targetMs === undefined ||
    snapshot.targetMs <= now ||
    snapshot.targetMs > now + MAX_ACTIVITY_MS
  )
    return null;
  const projection = widgetProjection(item, owner, showContent);
  return projection
    ? {
        ...projection.content,
        endsAt: snapshot.targetMs,
        labels: {
          refresh: t('nativeSurfaces.openToUpdate'),
          countdown: t('nativeSurfaces.countdownInProgress'),
          inProgress: t('nativeSurfaces.inProgress'),
        },
      }
    : null;
}
