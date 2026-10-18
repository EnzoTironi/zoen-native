import { File, Paths } from 'expo-file-system';
import { isAppCacheCopy } from './cache-paths';

/** Only invoke for a URI returned by an SDK operation we started, never user input. */
export function discardCacheCopy(uri: string | undefined): void {
  if (!uri || !isAppCacheCopy(uri, Paths.cache.uri)) return;
  const file = new File(uri);
  if (file.exists) file.delete();
}
