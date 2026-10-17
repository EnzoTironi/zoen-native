import { nativeLinkPath } from '../notifications/links';

export function redirectSystemPath({ path }: { path: string; initial: boolean }) {
  return nativeLinkPath(path);
}
