import { router } from 'expo-router';
import type { FileDto } from '../core/generated/FileDto';

/** Keep HTML in the isolated, owner-bound Site presentation rather than handing
 * it to a generic file viewer. Canonical permissions/bytes are re-read there. */
export function openSiteFile(info: FileDto | null, itemId: string, version?: number): boolean {
  if (process.env.EXPO_OS !== 'ios' || info?.mime !== 'text/html') return false;
  router.push({
    pathname: '/site/[id]',
    params: { id: itemId, ...(version ? { version: String(version) } : {}) },
  });
  return true;
}
